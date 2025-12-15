use core::alloc::Layout;
use core::sync::atomic::{AtomicUsize, Ordering};
use common::PAGE_SIZE;
use kernel_intf::info;
use crate::infra::disable_early_panic_phase;
use crate::{ds::*, hal};
use crate::sync::Spinlock;
use crate::mem::{allocate_memory, get_virtual_address, FixedAllocator, MapFetchType, PageDescriptor, Regions::*};
const INIT_STACK_SIZE: usize  = PAGE_SIZE * 2;
const INIT_GUARD_PAGE_SIZE: usize = PAGE_SIZE;
pub const TOTAL_STACK_SIZE: usize = INIT_STACK_SIZE + INIT_GUARD_PAGE_SIZE;

#[repr(C)]
#[cfg_attr(target_arch="x86_64", repr(align(4096)))]
#[cfg(feature = "stack_down")]
pub struct Stack {
    _guard_page: [u8; PAGE_SIZE],
    stack: [u8; INIT_STACK_SIZE]
}

#[repr(C)]
#[cfg_attr(target_arch="x86_64", repr(align(4096)))]
#[cfg(not(feature = "stack_down"))]
pub struct Stack {
    stack: [u8; INIT_STACK_SIZE],
    _guard_page: [u8; PAGE_SIZE]
}

struct CPUControlBlock {
    id: usize,
    worker_stack: &'static Stack,
    panic_base: usize
}

static KERN_INIT_STACK: Stack = Stack {
    stack: [0; INIT_STACK_SIZE],
    _guard_page: [0; PAGE_SIZE]
};

static CPU_ID: AtomicUsize = AtomicUsize::new(0);
static CPU_LIST: Spinlock<FixedList<CPUControlBlock, {Region4 as usize}>> = Spinlock::new(List::new());

pub fn init() {
    register_cpu();
}

pub fn register_cpu() -> usize {
    let cpu_id = CPU_ID.fetch_add(1, Ordering::Relaxed);
    let cb = if cpu_id == 0 {
        CPUControlBlock {
            id: cpu_id,
            worker_stack: &KERN_INIT_STACK,
            panic_base: hal::get_current_stack_base()
        }
    } else {
        // Allocate worker stack for the CPU
        let stack = unsafe {
            &*(allocate_memory(Layout::from_size_align(TOTAL_STACK_SIZE, PAGE_SIZE).unwrap()
            , PageDescriptor::VIRTUAL)
            .expect("Failed to allocate memory for CPU worker stack") as *mut Stack)
        };

        CPUControlBlock {
            id: cpu_id,
            worker_stack: stack,
            panic_base: get_stack_base(stack.stack.as_ptr() as usize)
        }
    };

    info!("Registered CPU with core_id:{} and stack:{:#X}", cpu_id, cb.worker_stack.stack.as_ptr() as usize);

    CPU_LIST.lock().add_node(cb).expect("Failed to add CPU control block to the list");

    disable_early_panic_phase();
    cpu_id
}

#[cfg(feature = "stack_down")]
#[inline(always)]
fn get_stack_base(stack_top: usize) -> usize {
    stack_top + INIT_STACK_SIZE
}

#[cfg(not(feature = "stack_down"))]
#[inline(always)]
fn get_stack_base(stack_top: usize) -> usize {
    stack_top
}

pub fn get_current_stack_base() -> usize {
    let core_id = hal::get_core();
    let cpu_list = CPU_LIST.lock();

    cpu_list.iter().find(|cb| cb.id == core_id)
        .and_then(|cb| Some(get_stack_base(cb.worker_stack.stack.as_ptr() as usize)))
        .expect("Invalid core ID!!")
}

pub fn get_panic_base() -> usize {
    let core_id = hal::get_core();
    let cpu_list = CPU_LIST.lock();

    cpu_list.iter().find(|cb| cb.id == core_id)
        .and_then(|cb| Some(cb.panic_base))
        .expect("Invalid core ID!!")
}

pub fn set_panic_base(base: usize) {
    let core_id = hal::get_core();
    let mut cpu_list = CPU_LIST.lock();

    if let Some(cb) = cpu_list.iter_mut().find(|cb| cb.id == core_id) {
        cb.panic_base = base;
    } else {
        panic!("Unable to find CPU control block for core_id:{}", core_id);
    }
}

pub fn relocate_cpu_init_stack() {
    let mut cpu_list = CPU_LIST.lock();

    if let Some(cb) = cpu_list.iter_mut().find(|cb| cb.id == 0) {
        // Update the stack address for the current CPU
        cb.worker_stack = unsafe {
            &*(get_virtual_address(cb.worker_stack as *const _ as usize, MapFetchType::Kernel)
            .expect("Failed to get virtual address for CPU init stack") as *const Stack)
        };

        info!("Relocated CPU init stack for main cpu to {:#X}", cb.worker_stack.stack.as_ptr() as usize);
    } else {
        panic!("Unable to find CPU control block for main cpu!!");
    }
}