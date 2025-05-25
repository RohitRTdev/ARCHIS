use core::cell::UnsafeCell;
mod asm;
pub struct Spinlock(UnsafeCell<u64>);

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
} 

pub fn init() {

}
