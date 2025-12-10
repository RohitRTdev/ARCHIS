#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(likely_unlikely)]

mod infra;
mod hal;
mod sync;
mod mem;
mod ds;
mod module;
mod logger;
mod cpu;
mod devices;

use core::alloc::Layout;
use kernel_intf::{info, debug};
use common::*;

extern crate alloc;
use alloc::{collections::BTreeMap, string::String, vec::Vec};


#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use crate::mem::{Allocator, PoolAllocator, Regions::*};
use crate::ds::*;

static BOOT_INFO: Once<BootInfo> = Once::new();

#[derive(Debug, PartialEq)]
enum RemapType {
    IdentityMapped,
    OffsetMapped(fn(usize))
}

#[derive(Debug)]
struct RemapEntry {
    value: MemoryRegion,
    map_type: RemapType,
    flags: u8
}

static INIT_FS: Once<BTreeMap<&'static str, &'static [u8]>> = Once::new();  
static REMAP_LIST: Spinlock<FixedList<RemapEntry, {Region3 as usize}>> = Spinlock::new(List::new());


#[test]
fn pool_alloc_test() {
    struct Sample1 {
        a: u32,
        b: u64,
        c: u8
    };

    struct Sample2 {
        a: u32,
        b: u64,
        c: u8,
        d: u64
    };

    let layout = Layout::new::<Sample1>();
    let obj1 = PoolAllocator::<Sample1>::alloc(layout).unwrap();
    debug!("Allocated Sample1 object at {:#?}", obj1);
    for _ in 0..10 {
        let obj = PoolAllocator::<Sample1>::alloc(layout).unwrap();
        debug!("Allocated Sample1 object at {:#?}", obj);
    }

    let layout = Layout::new::<Sample2>();
    let obj2 = PoolAllocator::<Sample2>::alloc(layout).unwrap();
    let layout = Layout::new::<Sample1>();
    unsafe {
        PoolAllocator::<Sample1>::dealloc(obj1, layout);
    }
    let obj3 = PoolAllocator::<Sample1>::alloc(layout).unwrap();
    let obj4 = PoolAllocator::<Sample1>::alloc(layout).unwrap();
    debug!("obj2 = {:#?}, obj3 = {:#?}, obj4 = {:#?}", obj2, obj3, obj4);
    let layout = Layout::new::<Sample2>();
    unsafe {
        PoolAllocator::<Sample2>::dealloc(obj2, layout);
    }

    let obj5 = PoolAllocator::<Sample2>::alloc(layout).unwrap();
    debug!("obj5 = {:#?}", obj5);
}


fn kern_main() {
    info!("Starting main kernel init");
    
#[cfg(feature = "acpi")]
    acpica::init();
    
    //hal::fire_interrupt();

    info!("Halting main core");
    hal::halt();
}

#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    BOOT_INFO.call_once(|| {
        *boot_info
    });   

    mem::setup_heap(); 
    devices::init();
    logger::init();
    
    info!("Starting aris");
    cpu::init();
    module::early_init();

    debug!("{:?}", *BOOT_INFO.get().unwrap());

    hal::init();
}

