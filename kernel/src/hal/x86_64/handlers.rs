use kernel_intf::{KError, debug, info};
use core::sync::atomic::{AtomicUsize, AtomicPtr, Ordering};
use core::ptr::NonNull;
use crate::cpu::{self, MAX_CPUS, PerCpu, general_interrupt_handler};
use crate::hal::{delay_ns, enable_scheduler_timer, get_core};
use crate::infra;
use crate::sync::{KSem, Spinlock};
use super::{lapic, timer};
use crate::mem::on_page_fault;
use super::lapic::{eoi, get_error};
use super::cpu::get_bsp_lapic_id;
use super::MAX_INTERRUPT_VECTORS;
use super::asm;
use crate::hal::halt;
use crate::devices::ioapic::add_redirection_entry;
use crate::ds::*;

pub const PAGE_FAULT_VECTOR: usize = 14;
pub const DOUBLE_FAULT_VECTOR: usize = 8;
pub const NMI_FAULT_VECTOR: usize = 2;
pub const SPURIOUS_VECTOR: usize = 32;
pub const YIELD_VECTOR: usize = 33;
pub const DEBUG_VECTOR: usize = 34;
pub const TIMER_VECTOR: usize = 35;
pub const ERROR_VECTOR: usize = 36;
pub const IPI_VECTOR: usize = 37;
const USER_VECTOR_START: usize = 38;

pub enum IPIRequestType {
    SchedChange,
    Shutdown
}

pub struct IPIRequest {
    req_type: IPIRequestType,
    core: usize,
    wait_mutex: KSem
}


static NEXT_AVAILABLE_VECTOR: AtomicUsize = AtomicUsize::new(USER_VECTOR_START);

const EXCEPTION_VECTOR_RANGE: usize = 32;

// This is set at init time and then never changed
pub static mut DEBUG_HANDLER_FN: Option<fn()> = None;

static PER_CPU_GLOBAL_CONTEXT: PerCpu<AtomicPtr<u8>> = PerCpu::new_with(
    [const {AtomicPtr::new(core::ptr::null_mut())}; MAX_CPUS]
);

static IPI_REQUESTS: Spinlock<DynList<IPIRequest>> = Spinlock::new(List::new());

static mut VECTOR_TABLE: [fn(usize); MAX_INTERRUPT_VECTORS] = [default_handler; MAX_INTERRUPT_VECTORS];
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

    unsafe {
        VECTOR_TABLE[vector as usize](vector as usize);
    }

    if vector as usize > DEBUG_VECTOR {
        eoi();
    }

    PER_CPU_GLOBAL_CONTEXT.local().load(Ordering::Relaxed) as *const CPUContext
}

fn default_handler(idx: usize) {
    panic!("Called default handler on vector: {}, {:?}", idx, unsafe{*(fetch_context() as *const CPUContext)});
}

pub fn init() {
    unsafe {
        for vector in 0..EXCEPTION_VECTOR_RANGE {
            VECTOR_TABLE[vector] = |idx| {
                // In these cases, we switch to different stack
                // Even though it's possible to still print the callstack, we don't do it for now
                if idx == NMI_FAULT_VECTOR || idx == DOUBLE_FAULT_VECTOR {
                    infra::disable_callstack();
                }

                if idx == DOUBLE_FAULT_VECTOR {
                    panic!("{} exception!\nPossible stack overflow??", EXCP_STRINGS[idx]);
                }
                else {
                    debug!("{:?}", unsafe {*(fetch_context() as *const CPUContext)});
                    panic!("{} exception!", EXCP_STRINGS[idx]);
                }
                
            };
        }

        VECTOR_TABLE[PAGE_FAULT_VECTOR] = page_fault_handler;

        for vector in USER_VECTOR_START..MAX_INTERRUPT_VECTORS {
            VECTOR_TABLE[vector] = general_interrupt_handler;
        }

        VECTOR_TABLE[SPURIOUS_VECTOR] = spurious_handler;
        VECTOR_TABLE[DEBUG_VECTOR] = debug_handler;
        VECTOR_TABLE[YIELD_VECTOR] = yield_handler;
        VECTOR_TABLE[TIMER_VECTOR] = timer_handler;
        VECTOR_TABLE[ERROR_VECTOR] = error_handler;
        VECTOR_TABLE[IPI_VECTOR] = ipi_handler;
    }
    info!("Initialized interrupt handlers");
}

// Interrupts must be disabled during this call
pub fn register_interrupt_handler(irq: usize, active_high: bool, is_edge_triggered: bool) -> usize {
    let vector = NEXT_AVAILABLE_VECTOR.fetch_add(1, Ordering::Relaxed);

    // We will tie up all IOAPIC interrupts to BSP
    add_redirection_entry(irq, get_bsp_lapic_id(), vector, active_high, is_edge_triggered);    
    
    vector
}

fn spurious_handler(_vector: usize) {
    debug!("Detected spurious interrupt!");
}

fn debug_handler(_vector: usize) {
    info!("Calling debug handler layer");
    unsafe {
        if let Some(handler) = DEBUG_HANDLER_FN {
            handler();
        }
    }
}

// It's fine to handle these without locks since CPU won't interrupt during this call
// This is true since we are already in interrupt handler and further interrupts are masked by current design
fn timer_handler(_vector: usize) {
    crate::sched::schedule();

    // Reload the timer
    lapic::setup_timer_value(timer::BASE_COUNT.load(Ordering::Relaxed) as u32);
}

// Do the same thing as timer handler, except we don't reload the timer register and we won't send EOI
fn yield_handler(_vector: usize) {
    crate::sched::schedule();
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

fn ipi_handler(_vector: usize) {
    // Search the requests list to find the first request aimed at this cpu
    let mut ipi_queue = IPI_REQUESTS.lock();
    let mut ipi_req = None;
    for req in ipi_queue.iter() {
        if req.core == get_core() {
            ipi_req = Some(NonNull::from(req));
            break;
        }
    }

    if let Some(req) = ipi_req {
        let req_info: &ListNode<IPIRequest> = unsafe {&*req.as_ptr()};
        match req_info.req_type {
            IPIRequestType::SchedChange => {
                debug!("Got IPI for new task...");
                enable_scheduler_timer();
                crate::sched::schedule();
            },
            IPIRequestType::Shutdown => {
                info!("Got IPI for shutdown...");
                halt();
            }
        }

        req_info.wait_mutex.signal();

        unsafe {
            ipi_queue.remove_node(req);
        }
    }
}

// Function should only be called after scheduler is up
pub fn notify_core(req_type: IPIRequestType, target_core: usize) -> Result<KSem, KError> {
    assert!(target_core < cpu::get_total_cores());
    
    let apic_id = super::get_apic_id(target_core);

    let wait_sem = KSem::new(0, 1);
    let req = IPIRequest {
        req_type, core: target_core, wait_mutex: wait_sem.clone()
    };

    IPI_REQUESTS.lock().add_node(req)?;

    lapic::send_ipi(apic_id as u32, IPI_VECTOR as u8);
    debug!("Issued IPI from core {} to core {}", super::get_core(), target_core);
    Ok(wait_sem)
}