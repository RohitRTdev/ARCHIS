use crate::debug;
use crate::sync::Spinlock;
use super::*;

const EXCEPTION_VECTOR_RANGE: usize = 32; 
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

#[derive(Debug)]
pub struct CPUContext {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rsi: u64,
    rdi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rbp: u64,
    rip: u64,
    cs: u64,
    rflags: u64,
    rsp: u64,
    ss: u64,
    rax: u64
}

#[no_mangle]
extern "C" fn global_interrupt_handler(vector: u64, cpu_context: *const CPUContext) {
    *crate::CUR_STACK_BASE.lock() = unsafe {
        asm::fetch_rbp()
    } as usize;

    VECTOR_TABLE.lock()[vector as usize](vector as usize);
}

fn default_handler(idx: usize) {
    debug!("Called default handler on vector:{}", idx);
}

pub fn init() {
    let mut vec_tbl = VECTOR_TABLE.lock();
    for vector in 0..EXCEPTION_VECTOR_RANGE {
        vec_tbl[vector] = |idx| {
            panic!("{} exception!", EXCP_STRINGS[idx]);
        };
    }
}