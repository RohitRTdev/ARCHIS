use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};

use crate::hal;

pub struct SpinlockGuard<'a, T> {
    lock: &'a hal::Spinlock,
    data: RefMut<'a, T>
}

impl<T> Deref for SpinlockGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

impl<T> DerefMut for SpinlockGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.data
    }
}


impl<T> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.unlock();
    }
}


pub struct Spinlock<T> {
    lock: hal::Spinlock,
    data: RefCell<T>
}

unsafe impl<T> Send for Spinlock<T>{}
unsafe impl<T> Sync for Spinlock<T>{}


impl<T> Spinlock<T> {

    pub const fn new(data: T) -> Self {
        Spinlock {
            lock: hal::Spinlock::new(),
            data: RefCell::new(data)
        }
    }

    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        self.lock.lock();
        SpinlockGuard { lock: &self.lock, data: self.data.borrow_mut()}
    }
}