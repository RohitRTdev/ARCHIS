mod serial_logger;
mod framebuffer_logger;

use core::fmt::Write;
use serial_logger::SERIAL;
use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::sync::Spinlock;
pub use framebuffer_logger::relocate_framebuffer;

#[macro_export]
macro_rules! println {
    () => {
        #[cfg(test)]
        {
            ::std::println!();
        }
        #[cfg(not(test))]
        {
            LOGGER.lock().write_fmt(::core::format_args!("\n")).unwrap();
        }
    };
    ($($arg:tt)*) => {
        #[cfg(test)]
        {
            ::std::println!($($arg)*);
        }
        #[cfg(not(test))]
        {
            use core::fmt::Write;
            let mut logger = crate::logger::LOGGER.lock();
            logger.write_fmt(::core::format_args!($($arg)*))
            .and_then(|_| logger.write_str("\n"))
            .unwrap();
        }
    };
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::println!("[INFO]: {}", format_args!($($arg)*));
    };
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::println!("[DEBUG]: {}", format_args!($($arg)*));
    };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

impl core::fmt::Write for KernelLogger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        // Write to serial
        SERIAL.lock().write(s);
        
        // Write to framebuffer
        FRAMEBUFFER_LOGGER.lock().write(s);
        
        Ok(())
    }
} 

pub struct KernelLogger(bool);
pub static LOGGER: Spinlock<KernelLogger> = Spinlock::new(KernelLogger(false));

pub fn init() {
    serial_logger::init();
    framebuffer_logger::init();
}

pub fn set_panic_mode() {
    LOGGER.lock().0 = true;
    FRAMEBUFFER_LOGGER.lock().clear_screen();
}
