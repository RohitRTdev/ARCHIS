use kernel_intf::{debug, info};
use core::sync::atomic::{AtomicUsize, Ordering};
use crate::cpu::{self, general_interrupt_handler};
use super::{lapic, timer};
use crate::sync::Spinlock;
use crate::mem::on_page_fault;
use super::lapic::{eoi, get_error};
use super::cpu::get_bsp_lapic_id;
use super::MAX_INTERRUPT_VECTORS;
use super::asm;
use crate::devices::ioapic::add_redirection_entry;

const PAGE_FAULT_VECTOR: usize = 14;
pub const SPURIOUS_VECTOR: usize = 32;
pub const TIMER_VECTOR: usize = 33;
pub const ERROR_VECTOR: usize = 34;
const USER_VECTOR_START: usize = 35;

static NEXT_AVAILABLE_VECTOR: AtomicUsize = AtomicUsize::new(USER_VECTOR_START);

const EXCEPTION_VECTOR_RANGE: usize = 32;
pub static mut KERNEL_TIMER_FN: Option<fn(*const u8)> = None;
static mut GLOBAL_CONTEXT: *const u8 = 0 as *const u8;

static VECTOR_TABLE: Spinlock<[fn(usize); MAX_INTERRUPT_VECTORS]> = Spinlock::new([default_handler; MAX_INTERRUPT_VECTORS]);
const UNDEFINED_STRING: &'static str = "Undefined";
const EXCP_STRINGS: [&'static str; EXCEPTION_VECTOR_RANGE] = [
    "Divide by zero",
    "Debug",
    "NMI",
    "Breakpoint",
    "Overflow",
    "BoundRange",
    "Invalid-opcode",
    "Device-not-available",
    "Double-fault",
    UNDEFINED_STRING,
    "Invalid TSS",
    "Segment-not-present",
    "Stack",
    "General protection",
    "Page fault",
    UNDEFINED_STRING,
    "x87-Floating-point",
    "Alignment-check",
    "Machine-check",
    "SIMD-floating-point",
    UNDEFINED_STRING,
    "Control-protection",
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING,
    UNDEFINED_STRING
];

#[derive(Debug, Clone, Copy)]
pub struct CPUContext {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
    vector: u64,
    rip: u64,
    cs: u64,
    rflags: u64
}

#[no_mangle]
extern "C" fn global_interrupt_handler(vector: u64, cpu_context: *const CPUContext) {
    unsafe {
        GLOBAL_CONTEXT = cpu_context as *const u8;
    }

    let saved_base = cpu::get_panic_base();
    cpu::set_panic_base(unsafe {
        asm::fetch_rbp() as usize
    });

    VECTOR_TABLE.lock()[vector as usize](vector as usize);

    if vector as usize > SPURIOUS_VECTOR {
        eoi();
    }

    cpu::set_panic_base(saved_base);
}

fn default_handler(idx: usize) {
    panic!("Called default handler on vector: {}", idx);
}

pub fn init() {
    let mut vec_tbl = VECTOR_TABLE.lock();
    for vector in 0..EXCEPTION_VECTOR_RANGE {
        vec_tbl[vector] = |idx| {
            panic!("{} exception!", EXCP_STRINGS[idx]);
        };
    }

    vec_tbl[PAGE_FAULT_VECTOR] = page_fault_handler;

    for vector in USER_VECTOR_START..MAX_INTERRUPT_VECTORS {
        vec_tbl[vector] = general_interrupt_handler;
    }

    vec_tbl[SPURIOUS_VECTOR] = spurious_handler;
    vec_tbl[TIMER_VECTOR] = timer_handler;
    vec_tbl[ERROR_VECTOR] = error_handler;
}

pub fn register_interrupt_handler(irq: usize, handler: fn(usize), active_high: bool, is_edge_triggered: bool) -> usize {
    let vector = NEXT_AVAILABLE_VECTOR.fetch_add(1, Ordering::Relaxed);

    // We will tie up all IOAPIC interrupts to BSP
    add_redirection_entry(irq, get_bsp_lapic_id(), vector, active_high, is_edge_triggered);    
    VECTOR_TABLE.lock()[vector] = handler;

    vector
}

fn spurious_handler(_vector: usize) {
    debug!("Detected spurious interrupt!");
}

// It's fine to handle these without locks since CPU won't interrupt during this call
// This is true since we are already in interrupt and further interrupts are masked by current design
fn timer_handler(_vector: usize) {
    unsafe {
        if let Some(handler) = KERNEL_TIMER_FN {
            handler(GLOBAL_CONTEXT);
        }
    }

    // Reload the timer
    lapic::setup_timer_value(timer::BASE_COUNT.load(Ordering::Acquire) as u32);
}

fn error_handler(_vector: usize) {
    info!("Error status register: {:#X}", get_error() & 0xff);
}

fn page_fault_handler(_vector: usize) {
    let fault_address = unsafe {
        asm::read_cr2()
    };

    on_page_fault(fault_address as usize);
}