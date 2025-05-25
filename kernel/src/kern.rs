#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]


mod infra;
mod hal;
mod lock;
mod mem;
mod ds;
mod logger;
use common::*;

#[cfg(test)]
mod tests;

pub fn kern_main() {
}

#[no_mangle]
extern "C" fn exported_function() {

}

#[no_mangle]
extern "C" fn kern_start(_boot_info: &BootInfo) -> ! {
    logger::init();  
    loop {}
}

