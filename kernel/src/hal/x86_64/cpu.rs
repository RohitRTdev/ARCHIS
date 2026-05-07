use core::sync::atomic::{AtomicBool, Ordering};
use crate::{cpu::{self, MAX_CPUS, PerCpu}};
use super::asm;
use super::lapic::*;

#[repr(C)]
pub struct CpuData {
    logical_id: u64,
    apic_id: u64,   
    current_worker_stack: u64,
    current_task_ptr: u64
}

const GS_BASE: u32 = 0xC0000101;
const KERNEL_GS_BASE: u32 = 0xC0000102; 

static PRE_INIT_PHASE: AtomicBool = AtomicBool::new(true);

// This list is initialized only once by each core during init, and later modified only by the structure's owning cpu
// scheduler (for the current task). 
// Cross cpu reference does exist, but it's only done to unmodified members (such as apic_id)
static mut CPU_LIST: PerCpu<CpuData> = PerCpu::new_with(
    [const {CpuData{logical_id: 0, apic_id: 0, current_worker_stack: 0, current_task_ptr: 0}}; MAX_CPUS]);

// This provides the logical cpu id
// BSP will always be given an ID of 0, followed by a contiguous assignment for remaining cpus
pub fn get_core() -> usize {
    // LAPIC is not initialized yet
    if PRE_INIT_PHASE.load(Ordering::Acquire) {
        return 0;
    }

    unsafe {
        get_per_cpu_data::<0>() as usize
    }
}

#[inline(always)]
pub unsafe fn get_per_cpu_data<const OFFSET: usize>() -> u64 {
    let per_cpu_data: u64;

    core::arch::asm!(
        "mov {}, gs:[{}]",
        out(reg) per_cpu_data,
        const OFFSET
    );

    per_cpu_data
} 

#[inline(always)]
pub unsafe fn set_per_cpu_data<const OFFSET: usize>(per_cpu_data: u64) {
    core::arch::asm!(
        "mov gs:[{}], {}",
        const OFFSET,
        in(reg) per_cpu_data
    );
} 

// Like the name suggests, this is to be run by each enabled core in the system once during init
pub fn init_per_cpu_data(core: usize) {
    let apic_id = get_lapic_id();
    
    let cpu_desc = unsafe {
        CPU_LIST.get_mut(core)
    };

    // Worker stack is a stack allocated in kernel mode per user thread
    // This will be used when a user thread calls into the kernel (via syscall)
    // It will null for kernel threads
    cpu_desc.current_worker_stack = 0;
    cpu_desc.logical_id = core as u64;
    cpu_desc.apic_id = apic_id as u64;

    unsafe {
        asm::wrmsr(KERNEL_GS_BASE, cpu_desc as *const _ as u64);
        asm::wrmsr(GS_BASE, cpu_desc as *const _ as u64);
    }

    PRE_INIT_PHASE.store(false, Ordering::Release);
}

pub fn get_per_cpu_kernel_base() -> u64 {
    unsafe {
        asm::rdmsr(GS_BASE)
    }
}

pub fn get_per_cpu_kernel_base_for_core(core: usize) -> u64 {
    unsafe {
        let cpu_desc = CPU_LIST.get(core);
        cpu_desc as *const _ as u64
    }
}

pub fn get_per_cpu_base() -> u64 {
    unsafe {
        asm::rdmsr(KERNEL_GS_BASE)
    }
}

pub fn set_per_cpu_base(new_base: u64) {
    unsafe {
        asm::wrmsr(KERNEL_GS_BASE, new_base)
    }
}

pub fn get_apic_id(core: usize) -> usize {
    unsafe {
        CPU_LIST.get(core).apic_id as usize
    }
}

pub fn get_bsp_lapic_id() -> usize {
    assert!(PRE_INIT_PHASE.load(Ordering::Relaxed) == false);

    unsafe {
        CPU_LIST.get(0).apic_id as usize
    }
}
