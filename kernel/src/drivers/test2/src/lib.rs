#![cfg_attr(not(test), no_std)]

#[kmod::init]
fn driver_init() {
    kernel_intf::info!("Initializing driver2...");
    unsafe {kernel_intf::exported_function();}
}

#[kmod::export]
fn get_test2() {
    kernel_intf::info!("Calling get_test2");
}
