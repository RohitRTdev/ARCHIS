mod framebuffer_logger;

use core::fmt::Write;
use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::devices::SERIAL;
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
macro_rules! level_print {
    ($level: literal, $($arg:tt)*) => {
        if crate::logger::LOGGER.lock().log_timestamp {
            $crate::println!("[{}]-[{}]-[{}]: {}", $level, crate::devices::read_rtc(), crate::hal::read_timestamp(), format_args!($($arg)*));
        } else {
            $crate::println!("[{}]-[{}]: {}", $level, crate::devices::read_rtc(), format_args!($($arg)*));
        }
    };
}


#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::level_print!("INFO", $($arg)*);
    };
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::level_print!("DEBUG", $($arg)*);
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

pub struct KernelLogger {
    pub panic_mode: bool,
    pub log_timestamp: bool
}

pub static LOGGER: Spinlock<KernelLogger> = Spinlock::new(KernelLogger { panic_mode: false,
                                                log_timestamp: false });

pub fn init() {
    framebuffer_logger::init();
}

pub fn set_panic_mode() {
    LOGGER.lock().panic_mode = true;
    FRAMEBUFFER_LOGGER.lock().clear_screen();
}

pub fn enable_timestamp() {
    LOGGER.lock().log_timestamp = true;
    info!("Enabled high resolution timestamp");
}
