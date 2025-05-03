#![no_main]
#![no_std]



use log::info;
use uefi::prelude::*;

extern crate alloc;
use alloc::vec;

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    info!("Starting bootloader...");
    let mut some_objects = vec![1,2,3,4];

    for i in 0..20 {
        info!("objects:{:?}", some_objects);
    }
    loop {}
    Status::SUCCESS
}
