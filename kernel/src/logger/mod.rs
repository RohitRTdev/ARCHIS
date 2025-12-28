mod framebuffer_logger;

use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::devices::uart;
pub use framebuffer_logger::relocate_framebuffer;

#[no_mangle]
pub extern "C" fn clear_screen() {
    FRAMEBUFFER_LOGGER.lock().clear_screen();
}

// It is important to ensure that caller holds the screen lock before calling this function
#[no_mangle]
extern "C" fn serial_print_ffi(s: *const u8, len: usize) {
    let s = unsafe {
        let slice = core::slice::from_raw_parts(s , len);
        core::str::from_utf8_unchecked(slice)
    }; 

    // Write to serial
    uart::SERIAL.lock().write(s);
    
    // Write to framebuffer
#[cfg(not(debug_assertions))]
    FRAMEBUFFER_LOGGER.lock().write(s);
}

pub fn init() {
    kernel_intf::init_logger();
    uart::init();
    framebuffer_logger::init();
    
    // We assume RTC always exists for PC-AT systems
    kernel_intf::enable_timestamp();
}
