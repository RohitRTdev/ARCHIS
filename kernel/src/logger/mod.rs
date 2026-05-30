mod framebuffer_logger;

use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::{devices::uart, logger::framebuffer_logger::flush_log};
use crate::hal;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
pub use framebuffer_logger::relocate_framebuffer;

static PANIC_MODE: AtomicBool = AtomicBool::new(false);
static PANIC_CORE: AtomicU8 = AtomicU8::new(0);

#[unsafe(no_mangle)]
pub extern "C" fn clear_screen() {
    FRAMEBUFFER_LOGGER.lock().clear_screen();
}

fn disable_cursor() {
    FRAMEBUFFER_LOGGER.lock().disable_cursor();
}

pub fn set_panic_mode(core: u8) {
    PANIC_MODE.store(true, Ordering::Release);
    PANIC_CORE.store(core, Ordering::Release);
    disable_cursor();
    clear_screen();
}

// It is important to ensure that caller holds the screen lock before calling this function
#[unsafe(no_mangle)]
extern "C" fn serial_print_ffi(s: *const u8, len: usize) {
    let s = unsafe {
        let slice = core::slice::from_raw_parts(s , len);
        core::str::from_utf8_unchecked(slice)
    }; 

    // During kernel panic, only allow the panicking core to log
    if !PANIC_MODE.load(Ordering::Acquire) || PANIC_CORE.load(Ordering::Acquire) == hal::get_core() as u8 {
        // Write to serial
        uart::SERIAL.lock().write(s);
        
        // Write to framebuffer
        FRAMEBUFFER_LOGGER.lock().write(s);
        flush_log();
    }
}

pub fn init() {
    kernel_intf::init_logger(env!("CARGO_PKG_NAME"));
    uart::init();
    framebuffer_logger::init();
    
    // We assume RTC always exists for PC-AT systems
    kernel_intf::enable_timestamp();
}
