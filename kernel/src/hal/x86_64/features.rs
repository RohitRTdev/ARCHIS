use crate::sync::{Once, Spinlock};
use crate::logger::debug;
use super::asm;

#[derive(Debug, Clone, Copy)]
pub struct CPUFeatures {
    pub umip: bool,
    pub smep: bool,
    pub smap: bool
}

pub static CPU_FEATURES: Once<Spinlock<CPUFeatures>> = Once::new();

fn check_bit(bit: u32, data: u32) -> bool {
    ((1 << bit) & data) != 0
}

fn cpuid(fn_number: u32, opt_fn_number: u32) -> [u32; 4] {
    let mut res =  [0; 4];
    unsafe {
        asm::cpuid(fn_number,  opt_fn_number, res.as_mut_ptr() as *mut u8);
    }

    res
}


pub fn init() {
    let fn_7_res = cpuid(7, 0);
    let is_umip = check_bit(2, fn_7_res[2]);
    let is_smep = check_bit(7, fn_7_res[1]);
    let is_smap = check_bit(20, fn_7_res[1]);

    CPU_FEATURES.call_once(|| {
        Spinlock::new(
            CPUFeatures { umip: is_umip, smep: is_smep, smap: is_smap }
        )
    });


    debug!("Features = {:?}", *CPU_FEATURES.get().unwrap().lock());
}