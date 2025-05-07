#![no_main]
#![no_std]


mod loader;
mod logger;

use uefi::prelude::*;

extern crate alloc;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    logger::init_logger();

    loader::list_fs();
    
    loop {}
    Status::SUCCESS
}
