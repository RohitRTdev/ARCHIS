#![cfg_attr(not(test), no_std)]

use common::StrRef;
use kernel_intf::exported_function;

static MODULE_NAME_STR: &'static str = env!("CARGO_PKG_NAME");

use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop{}
    //unsafe {
        //kernel_intf::panic_router(StrRef::from_str(MODULE_NAME_STR))
    //}
}

#[no_mangle]
extern "C" fn module_name() -> StrRef {
    StrRef::from_str(MODULE_NAME_STR)
}

#[no_mangle]
extern "C" fn module_init() -> i32 {
    let _boot_info: common::BootInfo;
    kernel_intf::init_logger(MODULE_NAME_STR);
    kernel_intf::enable_timestamp();
    kernel_intf::debug!("Initializing driver2...");
    unsafe {exported_function();}

    25
}

#[no_mangle]
extern "C" fn get_test2() {
    kernel_intf::debug!("Calling get_test2");
}
