use crate::info;
use crate::mem::MapFetchType;
use common::ptr_to_ref_mut;
mod asm;
mod utils;
mod features;
mod cpu_regs;
mod page_mapper;
mod tables;
mod handlers;
pub use utils::*;
pub use page_mapper::*;

const MAX_INTERRUPT_VECTORS: usize = 256;

pub fn disable_interrupts() -> bool {
    // RFLAGS register bit 9 is IF -> 1 is enabled
    (unsafe { asm::cli() } & (1 << 9)) != 0
}

pub fn enable_interrupts(int_status: bool) {
    // If interrupts were disabled previously, then don't enable them here
    if !int_status {
        return;
    }

    unsafe {
        asm::sti();
    }
}

pub use asm::read_port_u8;
pub use asm::write_port_u8;

use crate::KERN_INIT_STACK;

pub struct Spinlock(u64);

impl Spinlock {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn lock(&self) {
        unsafe {
            asm::acquire_lock(ptr_to_ref_mut(&self.0));
        }
    }
    
    pub fn unlock(&self) {
        unsafe {
            ptr_to_ref_mut::<_, u64>(&self.0).write(0);
        }
    }

    // Returns true if already locked, otherwise returns false and acquires lock
    // This is useful when you want to acquire the lock but not busy-wait
    pub fn try_lock(&self) -> bool {
        unsafe {
            asm::try_acquire_lock(ptr_to_ref_mut::<_, u64>(&self.0)) != 0
        }
    }
} 

#[cfg(not(test))]
#[inline(always)]
pub fn halt() -> ! {
    unsafe {
        asm::halt()
    }
}

#[cfg(debug_assertions)]
#[inline(always)]
pub fn get_current_stack_base() -> usize {
    unsafe {
        asm::fetch_rbp() as usize
    }
}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn get_current_stack_base() -> usize {
    // Cannot rely on rbp for optimized build, since compiler may not even use it for tracking frames
    unsafe {
        asm::fetch_rsp() as usize
    }
}

#[cfg(debug_assertions)]
pub fn unwind_stack(max_depth: usize, stack_base: usize, address: &mut [usize]) -> usize {
    let mut base = get_current_stack_base();
    let mut depth = 0;

    while depth < max_depth && stack_base >= base + 8 {
        let fn_addr = unsafe {*((base + 8) as *const u64)} as usize;
        address[depth] = fn_addr;
        
        base = unsafe {*(base as *const u64)} as usize;
        depth += 1;
    }

    depth
}

fn get_stack_base(stack_top: usize, stack_size: usize) -> usize {
    crate::mem::get_virtual_address(stack_top + stack_size, MapFetchType::Kernel).expect("Unexpected error. Stack virtual address not found!")
}

#[inline(always)]
pub fn fire_interrupt() {
    unsafe {
        asm::int();
    }
}


pub fn init() -> ! {
    info!("Starting platform initialization");

    features::init();
    cpu_regs::init();

    crate::mem::init();

    let stack_top = KERN_INIT_STACK.stack.as_ptr() as usize;
    let stack_base = get_stack_base(stack_top, crate::KERN_INIT_STACK_SIZE); 
    *crate::CUR_STACK_BASE.lock() = stack_base; 

    switch_to_new_address_space(page_mapper::get_kernel_pml4(), stack_base,
        crate::mem::get_virtual_address(tables::kern_addr_space_start as usize, MapFetchType::Kernel).expect("kern_addr_space_start virtual address not found!"));
}
