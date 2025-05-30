use core::panic::PanicInfo;

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let mut a = 5;
    
    loop {
        a += 2;
    }
}


#[cfg(not(test))]
#[no_mangle]
extern "C" fn panic_handler(info: &PanicInfo) -> ! {
    panic(info);
}