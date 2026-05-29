#![no_std]

use common::StrRef;
use test2_exports::get_test2;
use kernel_intf::exported_function;

static MODULE_NAME_STR: &'static str = env!("CARGO_PKG_NAME");

use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let mut a = 5;
    
    loop {
        a += 2;
    }
}

#[no_mangle]
extern "C" fn module_name() -> StrRef {
    StrRef::from_str(MODULE_NAME_STR)
}

#[no_mangle]
extern "C" fn module_init() -> i32 {
    let _boot_info: common::BootInfo;
    unsafe {exported_function();get_test2()}

    25
}

#[no_mangle]
extern "C" fn get_test1() {

}