#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]

mod infra;
mod hal;
mod sync;
mod mem;
mod ds;
mod module;
mod logger;

use common::*;
use logger::*;

#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};

static BOOT_INFO: Once<Spinlock<BootInfo>> = Once::new();

static KERN_STACK: [u8; PAGE_SIZE * 2] = [0; PAGE_SIZE * 2];

static CUR_STACK_BASE: Spinlock<usize> = Spinlock::new(0);


pub fn another_fn() {
    if *CUR_STACK_BASE.lock() != 25 {
        panic!("Sample testing");
    }

    *CUR_STACK_BASE.lock() = 75; 
}

pub fn kern_main() {
    another_fn();
}

#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    *CUR_STACK_BASE.lock() = hal::get_current_stack_base();
    
    BOOT_INFO.call_once(|| {
        Spinlock::new(*boot_info)
    });   

    
    logger::init(); 
    info!("Starting aris");
    info!("Early boot stack base:{}", *CUR_STACK_BASE.lock()); 

    module::early_init();

    info!("{:?}", *BOOT_INFO.get().unwrap().lock());
    kern_main();
    hal::halt();
}

