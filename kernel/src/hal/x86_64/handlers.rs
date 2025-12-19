use kernel_intf::{debug, info};
use core::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};
use crate::cpu::{self, PerCpu, MAX_CPUS, general_interrupt_handler};
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
pub const YIELD_VECTOR: usize = 33;
pub const TIMER_VECTOR: usize = 34;
pub const ERROR_VECTOR: usize = 35;
const USER_VECTOR_START: usize = 36;

static NEXT_AVAILABLE_VECTOR: AtomicUsize = AtomicUsize::new(USER_VECTOR_START);

const EXCEPTION_VECTOR_RANGE: usize = 32;

// This is set at init time and then never changed
pub static mut KERNEL_TIMER_FN: Option<fn()> = None;

static PER_CPU_GLOBAL_CONTEXT: PerCpu<AtomicPtr<u8>> = PerCpu::new_with(
    [const {AtomicPtr::new(core::ptr::null_mut())}; MAX_CPUS]
);

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
#[repr(C)]
struct CPUContext {
    pad: u64,
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
    rflags: u64,
    rsp: u64,
    ss: u64
}

// We require stack to be 16 byte aligned
const _: () = {
    assert!(core::mem::size_of::<CPUContext>() % 16 == 0);
};

impl CPUContext {
    fn new() -> Self {
        CPUContext { pad: 0, r15: 0, r14: 0, r13: 0, r12: 0, r11: 0, r10: 0, r9: 0, r8: 0, rbp: 0, rdi: 0, rsi: 0, 
            rdx: 0, rcx: 0, rbx: 0, rax: 0, vector: 0, rip: 0, cs: 0, rflags: 0, rsp: 0, ss: 0 
        }
    }
}

#[no_mangle]
extern "C" fn global_interrupt_handler(vector: u64, cpu_context: *const CPUContext) -> *const CPUContext {
    PER_CPU_GLOBAL_CONTEXT.local().store(cpu_context as *mut u8, Ordering::Relaxed);

    VECTOR_TABLE.lock()[vector as usize](vector as usize);

    if vector as usize > YIELD_VECTOR {
        eoi();
    }

    PER_CPU_GLOBAL_CONTEXT.local().load(Ordering::Relaxed) as *const CPUContext
}

fn default_handler(idx: usize) {
    panic!("Called default handler on vector: {}, {:?}", idx, unsafe{*(fetch_context() as *const CPUContext)});
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
    vec_tbl[YIELD_VECTOR] = yield_handler;
    vec_tbl[TIMER_VECTOR] = timer_handler;
    vec_tbl[ERROR_VECTOR] = error_handler;

    info!("Initialized interrupt handlers");
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
// This is true since we are already in interrupt handler and further interrupts are masked by current design
fn timer_handler(_vector: usize) {
    unsafe {
        if let Some(handler) = KERNEL_TIMER_FN {
            handler();
        }
    }

    // Reload the timer
    lapic::setup_timer_value(timer::BASE_COUNT.load(Ordering::Relaxed) as u32);
}

// Do the same thing as timer handler, except we don't reload the timer register and we won't send EOI
fn yield_handler(_vector: usize) {
    unsafe {
        if let Some(handler) = KERNEL_TIMER_FN {
            handler();
        }
    }
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

pub fn fetch_context() -> *const u8 {
    PER_CPU_GLOBAL_CONTEXT.local().load(Ordering::Acquire)
}

pub fn switch_context(new_context: *const u8) {
    PER_CPU_GLOBAL_CONTEXT.local().store(new_context as *mut u8, Ordering::Release);
}

pub fn create_kernel_context(handler: fn() -> !, stack_base: *mut u8) -> *const u8 {
    let mut sp = stack_base as usize;
    
    // 16 byte alignment is maintained since stack_base already aligned to 4096 bytes
    sp -= core::mem::size_of::<CPUContext>();

    let mut context = CPUContext::new(); 
    context.rip = handler as u64;
    context.rbp = stack_base.addr() as u64;
    context.rsp = stack_base.addr() as u64;
    
    // Kernel code + Kernel data
    context.cs = 0x8;
    context.ss = 0x10;
    context.rflags = unsafe {
        super::cpu_regs::INIT_RFLAGS
    };

    unsafe {
        (sp as *mut CPUContext).write(context);
    }
    sp as *const u8
}