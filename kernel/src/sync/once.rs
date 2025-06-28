use core::cell::UnsafeCell;
use crate::hal;

pub struct Once<T> {
    guard: hal::Spinlock,
    is_init: UnsafeCell<bool>,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T> Sync for Once<T> where T: Sync {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Self {
            guard: hal::Spinlock::new(),
            is_init: UnsafeCell::new(false),
            value: UnsafeCell::new(None)
        }
    }

    // Should not call from interrupt handler
    pub fn call_once<F>(&self, init: F)
    where
        F: FnOnce() -> T,
    {
        self.guard.lock();
        unsafe {
            *self.value.get() = Some(init());
            *self.is_init.get() = true;
        }
        self.guard.unlock();
    }

    pub fn get(&self) -> Option<&T> {
        if unsafe {*self.is_init.get()} == true {
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }
}