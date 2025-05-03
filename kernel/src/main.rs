#![no_std]
#![no_main]

mod infra;

#[no_mangle]
pub extern "C" fn kern_main() -> ! {
    let mut a  = 5;
    let ptr = 1000 as * mut u32;
    loop {
        unsafe {
            *ptr = 10;
        }
    }
}

