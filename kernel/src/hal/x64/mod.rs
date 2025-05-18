mod asm;

use core::cell::UnsafeCell;
pub struct Spinlock(UnsafeCell<u32>);


impl Spinlock {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(0))
    }

    pub fn lock(&self) {
        unsafe {
            //asm::acquire_lock(self.0.get());
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
