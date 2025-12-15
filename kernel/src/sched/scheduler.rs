use crate::hal::register_timer_fn;
use kernel_intf::info;
// This is in milliseconds
pub const QUANTUM: usize = 10;

static mut VALUE: usize = 0;

fn on_timer(context: *const u8) {
    unsafe {
        VALUE += 10;
        
        if VALUE >= 1000 {
            VALUE = 0;
            info!("One second completed!");
        }
    }
}

pub fn init() {
    register_timer_fn(on_timer);
}