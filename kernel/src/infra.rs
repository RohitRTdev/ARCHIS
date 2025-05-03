use core::panic::PanicInfo;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    let mut a = 5;
    
    loop {
        a += 2;
    }
}