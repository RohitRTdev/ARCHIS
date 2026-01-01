mod scheduler;
mod proc;
mod timer;

pub use proc::*;
pub use scheduler::*;
pub use timer::*;

use crate::hal::{disable_interrupts, enable_interrupts};

pub fn init() {
    proc::init();
    scheduler::init();
}