use core::cell::UnsafeCell;
mod asm;

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
            *self.0.get() = 0;
        }
    }

    // Returns true if already locked, otherwise returns false and acquires lock
    // This is useful when you want to acquire the lock but not busy-wait
    pub fn try_lock(&self) -> bool {
        unsafe {
            asm::try_acquire_lock(self.0.get()) != 0
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


pub fn init() {

}
