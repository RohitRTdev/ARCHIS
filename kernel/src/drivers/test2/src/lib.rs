#![no_std]

use common::StrRef;

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

#[cfg(test)]
unsafe fn exported_function() {}

#[cfg(not(test))]
#[link(name="aris")]
extern "C" {
    fn exported_function();
}


#[no_mangle]
extern "C" fn module_name() -> StrRef {
    StrRef::from_str(MODULE_NAME_STR)
}

#[no_mangle]
extern "C" fn module_init() -> i32 {
    let _boot_info: common::BootInfo;
    unsafe {exported_function();}

    25
}

#[no_mangle]
extern "C" fn get_test2() {

}
