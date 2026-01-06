mod scheduler;
mod proc;
mod timer;

pub use proc::*;
pub use scheduler::*;
pub use timer::*;

pub fn init() {
    proc::init();
    scheduler::init();
}