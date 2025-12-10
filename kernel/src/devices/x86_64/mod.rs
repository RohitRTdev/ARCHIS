mod rtc;
pub use rtc::*;

mod uart;
pub use uart::*;

mod hpet;

pub fn init() {
    uart::init();
}