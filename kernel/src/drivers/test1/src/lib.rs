#![no_std]

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
extern "C" fn test_function() -> i32 {
    let _boot_info: common::BootInfo;
    unsafe {exported_function();}

    25
}