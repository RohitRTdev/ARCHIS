#![cfg_attr(not(test), no_std)]

use kernel_intf::exported_function;

#[kmod::init]
fn driver_init() {
    let _boot_info: common::BootInfo;
    kernel_intf::info!("Driver1 initializing...");
    unsafe {exported_function();test2::get_test2()}
}
