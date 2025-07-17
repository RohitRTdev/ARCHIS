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

extern crate alloc;
use alloc::vec::Vec;
use alloc::string::String;


#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use crate::mem::{FixedAllocator, Regions::*};
use crate::ds::*;

static BOOT_INFO: Once<BootInfo> = Once::new();

const KERN_INIT_STACK_SIZE: usize  = PAGE_SIZE * 2;

#[cfg_attr(target_arch="x86_64", repr(align(4096)))]
struct Stack {
    stack: [u8; KERN_INIT_STACK_SIZE],
    _guard_page: [u8; PAGE_SIZE]
}

#[derive(Debug, PartialEq)]
enum RemapType {
    IdentityMapped,
    OffsetMapped(fn(usize))
}

#[derive(Debug)]
struct RemapEntry {
    value: MemoryRegion,
    map_type: RemapType
}

static KERN_INIT_STACK: Stack = Stack {
    stack: [0; KERN_INIT_STACK_SIZE],
    _guard_page: [0; PAGE_SIZE]
};

static REMAP_LIST: Spinlock<List<RemapEntry, FixedAllocator<ListNode<RemapEntry>, {Region3 as usize}>>> = Spinlock::new(List::new());
static CUR_STACK_BASE: Spinlock<usize> = Spinlock::new(0);

fn kern_main() {
    info!("Starting main kernel init");

    {
        let mut list = Vec::new();
        list.push(1);
        list.push(3);
        list.push(23);
        list.push(23);
        list.push(23);
        list.push(23);
        list.push(23);
        list.push(23);
        debug!("List={:?}", list);
        list.remove(3);
        list.remove(2);
        debug!("List={:?}", list);


        let mut s = String::from("Heap allocated string test!!");
        debug!("String test = {}", s);
        s.insert_str(4, " string");
        debug!("String test after insertion = {}", s);
    }

    //hal::fire_interrupt();

    info!("Halting main core");
    hal::halt();
}

#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    hal::disable_interrupts();
    *CUR_STACK_BASE.lock() = hal::get_current_stack_base();
    
    BOOT_INFO.call_once(|| {
        *boot_info
    });   

    mem::setup_heap(); 
    logger::init();
    info!("Starting aris");
    info!("Early boot stack base:{:#X}", *CUR_STACK_BASE.lock()); 

    module::early_init();

    debug!("{:?}", BOOT_INFO.get().unwrap());

    hal::init();
}

