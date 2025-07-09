#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]

mod infra;
mod hal;
mod sync;
mod mem;
mod ds;
mod module;
mod logger;
mod error;

use common::*;
use logger::*;

#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use crate::mem::{FixedAllocator, Regions::*};
use crate::ds::*;

static BOOT_INFO: Once<Spinlock<BootInfo>> = Once::new();

const KERN_INIT_STACK_SIZE: usize  = PAGE_SIZE * 2;

#[cfg_attr(target_arch="x86_64", repr(align(4096)))]
struct Stack {
    stack: [u8; KERN_INIT_STACK_SIZE],
    _guard_page: [u8; PAGE_SIZE]
}

#[derive(Debug)]
struct RemapEntry {
    value: MemoryRegion,
    is_identity_mapped: bool
}

static KERN_INIT_STACK: Stack = Stack {
    stack: [0; KERN_INIT_STACK_SIZE],
    _guard_page: [0; PAGE_SIZE]
};


static REMAP_LIST: Spinlock<List<RemapEntry, FixedAllocator<ListNode<RemapEntry>, {Region3 as usize}>>> = Spinlock::new(List::new());
static CUR_STACK_BASE: Spinlock<usize> = Spinlock::new(0);

extern "C" fn kern_main() {
    info!("Starting main kernel init");
    hal::halt();
}

#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    hal::disable_interrupts();
    *CUR_STACK_BASE.lock() = hal::get_current_stack_base();
    
    BOOT_INFO.call_once(|| {
        Spinlock::new(*boot_info)
    });   

    
    logger::init();
    info!("Starting aris");
    info!("Early boot stack base:{:#X}", *CUR_STACK_BASE.lock()); 

    module::early_init();

    debug!("{:?}", *BOOT_INFO.get().unwrap().lock());

    hal::init();
}

