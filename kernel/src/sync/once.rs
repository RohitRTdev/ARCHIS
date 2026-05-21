use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::hal;

pub struct Once<T> {
    guard: hal::Spinlock,
    is_init: AtomicBool,
    value: UnsafeCell<Option<T>>,
}

unsafe impl<T> Sync for Once<T> where T: Send {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Self {
            guard: hal::Spinlock::new(),
            is_init: AtomicBool::new(false),
            value: UnsafeCell::new(None)
        }
    }

    // Should not call from interrupt handler
    pub fn call_once<F>(&self, init: F)
    where
        F: FnOnce() -> T,
    {
        // Fast path: already initialised, no lock needed.
        if self.is_init.load(Ordering::Acquire) {
            return;
        }

        // Disable interrupts BEFORE acquiring the guard. If an interrupt fired
        // while we held the guard and the handler tried to call_once() on the
        // same Once, the non-reentrant raw spinlock would deadlock.
        let int_status = hal::disable_interrupts();
        self.guard.lock();

        if !self.is_init.load(Ordering::Acquire) {
            unsafe {
                *self.value.get() = Some(init());
            }
            // Release-store pairs with the Acquire-loads in is_init()/get()
            // and in the fast path above, ensuring that any thread which
            // observes is_init == true also observes the fully written value.
            self.is_init.store(true, Ordering::Release);
        }

        self.guard.unlock();
        hal::enable_interrupts(int_status);
    }

    pub fn get(&self) -> Option<&T> {
        if self.is_init.load(Ordering::Acquire) {
            unsafe { (*self.value.get()).as_ref() }
        } else {
            None
        }
    }
}
