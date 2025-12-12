use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};

#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

use kernel_intf::Lock;
use crate::hal;
use common::ptr_to_ref_mut;

// This assumption is used by Lock variable in kernel_intf
const _: () = {
    assert!(core::mem::size_of::<hal::Spinlock>() == 8);
};

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

#[cfg(not(test))]
impl<T> Drop for SpinlockGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.unlock();
        hal::enable_interrupts(self.int_status);
    }
}

pub struct Spinlock<T> {
#[cfg(not(test))] 
    pub lock: hal::Spinlock,
#[cfg(test)]
    pub lock: Mutex<u32>,
    data: RefCell<T>
}

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

#[no_mangle]
extern "C" fn create_spinlock(lock: &mut Lock) {
    let val = hal::Spinlock::new();

    unsafe {
        ptr_to_ref_mut::<_, hal::Spinlock>(&lock.lock).write(val);
    }
}

#[no_mangle]
extern "C" fn acquire_spinlock(lock: &mut Lock) {
    unsafe {
        let stat = hal::disable_interrupts();
        (*ptr_to_ref_mut::<_, hal::Spinlock>(&lock.lock)).lock(); 
        
        lock.int_status = stat;
    }
}

#[no_mangle]
extern "C" fn release_spinlock(lock: &mut Lock) {
    unsafe {
        (*ptr_to_ref_mut::<_, hal::Spinlock>(&lock.lock)).unlock(); 
        hal::enable_interrupts(lock.int_status);
    }
}
        
