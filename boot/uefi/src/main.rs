#![no_std]
#![no_main]


const PAGE_SIZE: usize = 4096;

mod loader;
mod logger;

use uefi::prelude::*;
use log::{info, debug};
use core::panic::PanicInfo;
use core::alloc::Layout;
use uefi::{Identify, proto::media::fs::SimpleFileSystem};

extern crate alloc;

#[no_mangle]
extern "Rust" fn loader_alloc(layout: Layout) -> *mut u8 {
    assert!(layout.align() <= PAGE_SIZE, "Cannot satisfy memory alignment constraint of more than 4096 bytes!!");
    debug!("Requesting memory allocation for {:?}", layout);

    let pages = common::ceil_div(layout.size(), PAGE_SIZE);

    boot::allocate_pages(boot::AllocateType::AnyPages, boot::MemoryType::LOADER_DATA, pages).expect(
        "Memory allocation failed!!"
    ).as_ptr() 
}


#[entry]
fn main() -> Status {
    logger::init_logger();
    
    // First get all available handles for partition information protocol
    info!("Fetching FAT32 formatted partitions...");
    let supported_handles = boot::locate_handle_buffer(boot::SearchType::ByProtocol(&SimpleFileSystem::GUID)).unwrap();

    let root_partition = loader::list_fs(&supported_handles);
    loader::load_init_fs(root_partition);
    loop {}
    Status::SUCCESS
}


#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    common::println!("[PANIC!!!]: {}\r", info.message());
    loop{}
}