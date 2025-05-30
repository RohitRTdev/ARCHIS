mod serial_logger;

use core::fmt::Write;
use serial_logger::SERIAL;
use crate::lock::Spinlock;

impl core::fmt::Write for KernelLogger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        SERIAL.lock().write(s);
        Ok(())
    }
} 

struct KernelLogger;

impl log::Log for Spinlock<KernelLogger> {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info 
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            let _ = write!(&mut *self.lock(), "[{}]: {}\n", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

static LOGGER: Spinlock<KernelLogger> = Spinlock::new(KernelLogger{});

pub fn init() {
    serial_logger::init();
    log::set_logger(&LOGGER).unwrap();
    log::set_max_level(log::LevelFilter::Info);
}