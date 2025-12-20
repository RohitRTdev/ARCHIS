#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(likely_unlikely)]
#![feature(allocator_api)]

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
use crate::hal::{delay_ns, disable_interrupts, enable_interrupts, read_port_u8};
use crate::mem::Regions::*;
use crate::ds::*;
use crate::sync::KSem;

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
    install_interrupt_handler(1, key_notifier, true, true);
    
    sched::init();
    sched::create_task(producer).unwrap();
    sched::create_task(consumer).unwrap();

    info!("Main task going to sleep"); 
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

use alloc::vec::Vec;
static DATA_QUEUE: Spinlock<Vec<u8>> = Spinlock::new(Vec::new());
static WRITE_EVENT: KSem = KSem::new(0, 1);
static KEYBOARD_EVENT: KSem = KSem::new(0, 1);

const SCANCODE_SET1_TO_ASCII: [Option<u8>; 58] = [
    /* 0x00 */ None,
    /* 0x01 */ Some(0x1B), // Esc
    /* 0x02 */ Some(b'1'),
    /* 0x03 */ Some(b'2'),
    /* 0x04 */ Some(b'3'),
    /* 0x05 */ Some(b'4'),
    /* 0x06 */ Some(b'5'),
    /* 0x07 */ Some(b'6'),
    /* 0x08 */ Some(b'7'),
    /* 0x09 */ Some(b'8'),
    /* 0x0A */ Some(b'9'),
    /* 0x0B */ Some(b'0'),
    /* 0x0C */ Some(b'-'),
    /* 0x0D */ Some(b'='),
    /* 0x0E */ Some(0x08), // Backspace
    /* 0x0F */ Some(b'\t'),

    /* 0x10 */ Some(b'q'),
    /* 0x11 */ Some(b'w'),
    /* 0x12 */ Some(b'e'),
    /* 0x13 */ Some(b'r'),
    /* 0x14 */ Some(b't'),
    /* 0x15 */ Some(b'y'),
    /* 0x16 */ Some(b'u'),
    /* 0x17 */ Some(b'i'),
    /* 0x18 */ Some(b'o'),
    /* 0x19 */ Some(b'p'),
    /* 0x1A */ Some(b'['),
    /* 0x1B */ Some(b']'),
    /* 0x1C */ Some(b'\n'),
    /* 0x1D */ None, // Ctrl

    /* 0x1E */ Some(b'a'),
    /* 0x1F */ Some(b's'),
    /* 0x20 */ Some(b'd'),
    /* 0x21 */ Some(b'f'),
    /* 0x22 */ Some(b'g'),
    /* 0x23 */ Some(b'h'),
    /* 0x24 */ Some(b'j'),
    /* 0x25 */ Some(b'k'),
    /* 0x26 */ Some(b'l'),
    /* 0x27 */ Some(b';'),
    /* 0x28 */ Some(b'\''),
    /* 0x29 */ Some(b'`'),

    /* 0x2A */ None, // Left Shift
    /* 0x2B */ Some(b'\\'),
    /* 0x2C */ Some(b'z'),
    /* 0x2D */ Some(b'x'),
    /* 0x2E */ Some(b'c'),
    /* 0x2F */ Some(b'v'),
    /* 0x30 */ Some(b'b'),
    /* 0x31 */ Some(b'n'),
    /* 0x32 */ Some(b'm'),
    /* 0x33 */ Some(b','),
    /* 0x34 */ Some(b'.'),
    /* 0x35 */ Some(b'/'),
    /* 0x36 */ None, // Right Shift
    /* 0x37 */ Some(b'*'),
    /* 0x38 */ None, // Alt
    /* 0x39 */ Some(b' ')
];

fn key_notifier(_: usize) {
    KEYBOARD_EVENT.signal();
}

fn producer() -> ! {
    info!("Starting producer task");
    clear_keyboard_output_buffer();
    DATA_QUEUE.lock().reserve(256);

    unsafe {
        loop {
            KEYBOARD_EVENT.wait().unwrap();

            while read_port_u8(0x64) & 0x01 != 0 {
                let value = read_port_u8(0x60);
                DATA_QUEUE.lock().push(value);
            }

            WRITE_EVENT.signal();
        }
    }
}

fn consumer() -> ! {
    info!("Starting consumer task");
    info!("Primitive terminal... You may type");

    loop {
        WRITE_EVENT.wait().unwrap();
        {
            let mut data = DATA_QUEUE.lock();
            let mut idx = 0;
            while idx < data.len() {
                let scancode = data[idx];
                idx += 1;

                // Ignore key releases
                if scancode & 0x80 != 0 {
                    continue;
                }

                // Ignore extended scancodes
                if scancode == 0xE0 {
                    continue;
                }

                if let Some(ascii) = SCANCODE_SET1_TO_ASCII
                    .get(scancode as usize)
                    .and_then(|v| *v) {
                    kernel_intf::print!("{}", ascii as char);
                }
            }

            data.clear();
        }   
    }
}

