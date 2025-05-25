#![no_std]
#![no_main]

mod loader;
mod logger;

use uefi::prelude::*;
use core::panic::PanicInfo;

extern crate alloc;

#[entry]
fn main() -> Status {
    logger::init_logger();

    loader::list_fs();
    loop {}
    Status::SUCCESS
}


#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    logger::info!("Panicked!!");
    loop{}
}