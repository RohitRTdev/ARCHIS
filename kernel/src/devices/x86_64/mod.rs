mod rtc;
pub use rtc::*;

mod uart;
pub use uart::*;

mod hpet;
pub use hpet::*;

pub fn early_init() {
    uart::init();
}

pub fn init() {
#[cfg(feature = "acpi")]
    hpet::init();
}