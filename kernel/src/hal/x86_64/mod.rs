use kernel_intf::info;
use crate::mem::MapFetchType;
use core::sync::atomic::Ordering;
use core::cell::UnsafeCell;
mod asm;
mod utils;
mod features;
mod cpu_regs;
mod page_mapper;
mod tables;
mod handlers;
mod cpu;
mod timer;
mod lapic;
pub use cpu::*;
pub use utils::*;
pub use page_mapper::*;
pub use handlers::*;
pub use timer::delay_ns;

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

pub struct Spinlock(UnsafeCell<u64>);

impl Spinlock {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(0))
    }

    pub fn lock(&self) {
        unsafe {
            asm::acquire_lock(self.0.get());
        }
    }
    
    pub fn unlock(&self) {
        unsafe {
            // In x86_64, writes follow release semantics (Since no earlier load/store is reordered with a later store)
            self.0.get().write(0);
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

#[cfg(not(test))]
#[inline(always)]
pub fn sleep() -> ! {
    unsafe {
        asm::sleep()
    }
}

#[cfg(not(test))]
#[inline(always)]
pub fn yield_cpu() {
    unsafe {
        asm::fire_yield_interrupt();
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
        let prev_base = base;
        let fn_addr = unsafe {*((base + 8) as *const u64)} as usize;
        base = unsafe {*(base as *const u64)} as usize;
        
        if base <= prev_base {
            break;
        }

        address[depth] = fn_addr;
        depth += 1;
    }

    depth
}

pub fn init() -> ! {
    info!("Starting platform initialization");

    features::init();
    cpu_regs::init();
    
    crate::mem::init();

    let stack_base = crate::mem::get_virtual_address(crate::cpu::get_current_stack_base(), MapFetchType::Kernel)
    .expect("Unexpected error. Stack virtual address not found!");

    switch_to_new_address_space(page_mapper::get_kernel_pml4(), stack_base,
        crate::mem::get_virtual_address(tables::kern_addr_space_start as *const () as usize, MapFetchType::Kernel).expect("kern_addr_space_start virtual address not found!"));
}

// This function should only be called once during init
// Tells hal that kernel is ready to handle timer interrupts
pub fn register_timer_fn(handler: fn()) {
    unsafe {
        handlers::KERNEL_TIMER_FN = Some(handler);
    }

    lapic::enable_timer(timer::BASE_COUNT.load(Ordering::Acquire) as u32);
}
