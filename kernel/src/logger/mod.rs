mod framebuffer_logger;

use core::ffi::CStr;
use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::devices::SERIAL;
use crate::hal;
pub use framebuffer_logger::relocate_framebuffer;

static SCREEN_LOCK: hal::Spinlock = hal::Spinlock::new();

#[no_mangle]
extern "C" fn acquire_screen_lock() -> bool {
    let stat = hal::disable_interrupts();
    SCREEN_LOCK.lock();

    stat
}

#[no_mangle]
extern "C" fn release_screen_lock(int_status: bool) {
    SCREEN_LOCK.unlock();

    hal::enable_interrupts(int_status);
}


// Make sure that screen lock is held before call
#[no_mangle]
extern "C" fn clear_screen() {
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
    SERIAL.lock().write(s);
    
    // Write to framebuffer
    FRAMEBUFFER_LOGGER.lock().write(s);
}

pub fn init() {
    kernel_intf::init_logger();
    framebuffer_logger::init();
}
