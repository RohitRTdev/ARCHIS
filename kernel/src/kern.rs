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

#[derive(Debug)]
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

fn kern_main() {
    info!("Starting main kernel init");
    
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
    devices::early_init();
    logger::init();
    devices::init();
    
    info!("Starting aris");
    cpu::init();
    module::early_init();

    debug!("{:?}", *BOOT_INFO.get().unwrap());

    hal::init();
}

