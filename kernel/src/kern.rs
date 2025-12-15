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
mod sched;

use kernel_intf::{info, debug};
use common::*;

extern crate alloc;
use alloc::collections::BTreeMap;


#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use cpu::install_interrupt_handler;
use crate::hal::{disable_interrupts, read_port_u8};
use crate::mem::Regions::*;
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

fn clear_keyboard_output_buffer() {
    unsafe {
        while read_port_u8(0x64) & 0x01 != 0 {
            let _ = read_port_u8(0x60);
        }
    }
}

fn kern_main() {
    info!("Starting main kernel init");

    // Sample invocation to test out interrupt subsystem
    clear_keyboard_output_buffer();
    install_interrupt_handler(1, |vec| {
        let scancode = unsafe {
            read_port_u8(0x60)
        };
        info!("Notified of keypress via vector {}. Scancode={}", vec, scancode);
    }, true, true);
    
    sched::init();

    info!("Main core going to sleep");
    hal::sleep();
}

#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    disable_interrupts();
    BOOT_INFO.call_once(|| {
        *boot_info
    });   

    mem::setup_heap();
    logger::init();

    info!("Starting aris");
    devices::init();
    cpu::init();
    module::early_init();

    debug!("{:?}", *BOOT_INFO.get().unwrap());

    hal::init();
}

