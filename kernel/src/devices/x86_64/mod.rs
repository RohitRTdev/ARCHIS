mod rtc;
pub use rtc::*;

mod uart;
pub use uart::*;

pub fn init() {
    uart::init();
}