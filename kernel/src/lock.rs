use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};

#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

use crate::hal;

pub struct SpinlockGuard<'a, T> {
#[cfg(not(test))]
    lock: &'a hal::Spinlock,
#[cfg(test)]
    _lock: MutexGuard<'a, u32>,
    int_status: bool,
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
#[cfg(not(test))]
        self.lock.unlock();
        hal::enable_interrupts(self.int_status);
    }
}

pub struct Spinlock<T> {
#[cfg(not(test))] 
    lock: hal::Spinlock,
#[cfg(test)]
    lock: Mutex<u32>,
    data: RefCell<T>
}

unsafe impl<T> Send for Spinlock<T>{}
unsafe impl<T> Sync for Spinlock<T>{}


impl<T> Spinlock<T> {

    pub const fn new(data: T) -> Self {
        Spinlock {
#[cfg(not(test))]
            lock: hal::Spinlock::new(),
#[cfg(test)]
            lock: Mutex::new(0),
            data: RefCell::new(data)
        }
    }

#[cfg(not(test))]
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let int_status = hal::disable_interrupts();
        self.lock.lock();
        SpinlockGuard { lock: &self.lock, int_status, data: self.data.borrow_mut()}
    }

#[cfg(test)]
    pub fn lock(&self) -> SpinlockGuard<'_, T> {
        let guard = self.lock.lock().unwrap();
        SpinlockGuard { _lock: guard, int_status: false, data: self.data.borrow_mut()}
    }
}