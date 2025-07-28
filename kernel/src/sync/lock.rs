use core::cell::{RefCell, RefMut};
use core::ops::{Deref, DerefMut};

use core::ptr::NonNull;
#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

use kernel_intf::Lock;
use core::alloc::Layout;
use crate::devices::SERIAL;
use crate::hal;
use crate::mem::{Allocator, FixedAllocator, Regions::*};

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


// We're using fixed allocator since spinlocks will be required from early boot process
#[no_mangle]
extern "C" fn create_spinlock(lock: &mut Lock) {
    let val = hal::Spinlock::new();
    let ptr = FixedAllocator::<hal::Spinlock, {Region6 as usize}>::alloc(Layout::new::<hal::Spinlock>()).unwrap();

    unsafe {
        ptr.write(val);
    }

    lock.lock = ptr.as_ptr() as *mut u8;
}

#[no_mangle]
extern "C" fn acquire_spinlock(lock: &mut Lock) {
    unsafe {
        let stat = hal::disable_interrupts();
        (*(lock.lock as *mut hal::Spinlock)).lock(); 
        
        lock.int_status = stat;
    }
}

#[no_mangle]
extern "C" fn release_spinlock(lock: &mut Lock) {
    unsafe {
        (*(lock.lock as *mut hal::Spinlock)).unlock(); 
        hal::enable_interrupts(lock.int_status);
    }
}
        
#[no_mangle]
extern "C" fn delete_spinlock(lock: &mut Lock) {
    unsafe {
        FixedAllocator::<hal::Spinlock, {Region6 as usize}>::dealloc(NonNull::new(lock.lock as *mut hal::Spinlock).unwrap(), Layout::new::<hal::Spinlock>());
    }
}
        
