mod serial_logger;
mod framebuffer_logger;

use core::fmt::Write;
use serial_logger::SERIAL;
use framebuffer_logger::FRAMEBUFFER_LOGGER;
use crate::sync::Spinlock;

pub use log::{debug, info};

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

impl log::Log for Spinlock<KernelLogger> {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level() 
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let _ = write!(&mut *self.lock(), "[{}]: {}\n", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub static LOGGER: Spinlock<KernelLogger> = Spinlock::new(KernelLogger(false));

pub fn init() {
    serial_logger::init();
    framebuffer_logger::init();
    log::set_logger(&LOGGER).unwrap();

#[cfg(debug_assertions)]
    log::set_max_level(log::LevelFilter::Debug);

#[cfg(not(debug_assertions))]
    log::set_max_level(log::LevelFilter::Info);

}

pub fn set_panic_mode() {
    LOGGER.lock().0 = true;
    FRAMEBUFFER_LOGGER.lock().clear_screen();
}
