#![no_std]
#![feature(generic_const_exprs)]

mod infra;
mod hal;
mod lock;
mod mem;
mod ds;
mod logger;
use common::*;

fn kern_main() {

}


#[no_mangle]
extern "C" fn kern_start(boot_info: &BootInfo) -> ! {
    logger::init();   
    loop {}
}

