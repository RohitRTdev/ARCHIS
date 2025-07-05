use core::marker::PhantomData;

use super::{asm, features};
use crate::logger::debug;

pub struct CR0;
pub struct CR4;
pub struct RFLAGS;
pub struct EFER;

struct CPUReg<T: Reg> {
    _mark: PhantomData<T>
}

trait Reg {
    fn read() -> u64;
    unsafe fn write(data: u64);
}

impl<T: Reg> CPUReg<T> {
    pub unsafe fn init(data: u64) {
        T::write(data);
    }

    pub unsafe fn set(mask: u64) {
        let mut reg = T::read();
        reg |= mask;

        T::write(reg);
    }

    pub unsafe fn clear(mask: u64) {
        let mut reg = T::read(); 
        reg &= !mask;

        T::write(reg);
    }
}

impl Reg for CR0 {
    unsafe fn write(data: u64) {
        asm::write_cr0(data);
    }

    fn read() -> u64 {
        unsafe {
            asm::read_cr0()
        }
    }
}

impl Reg for CR4 {
    unsafe fn write(data: u64) {
        asm::write_cr4(data);
    }
    
    fn read() -> u64 {
        unsafe {
            asm::read_cr4()
        }
    }
}

impl Reg for RFLAGS {
    unsafe fn write(data: u64) {
        asm::write_rflags(data);
    }
    
    fn read() -> u64 {
        unsafe {
            asm::read_rflags()
        }
    }
}

impl Reg for EFER {
    unsafe fn write(data: u64) {
        asm::wrmsr(EFER::ADDRESS, data);
    }
    fn read() -> u64 {
        unsafe {
            asm::rdmsr(EFER::ADDRESS)
        }
    }
}

impl CR0 {
    pub const PE: u64 = 1 << 0;
    pub const ET: u64 = 1 << 4;
    pub const NE: u64 = 1 << 5;
    pub const WP: u64 = 1 << 16;
    pub const PG: u64 = 1 << 31;
}

impl CR4 {
    pub const PAE: u64 = 1 << 5;
    pub const PGE: u64 = 1 << 7;
    pub const PCE: u64 = 1 << 8;
    pub const UMIP: u64 = 1 << 11;
    pub const SMEP: u64 = 1 << 20;
    pub const SMAP: u64 = 1 << 21;
}

impl RFLAGS {
    pub const IOPL: u64 = 3 << 12;
    pub const AC: u64 = 1 << 18;
}

impl EFER {
    pub const ADDRESS: u32 = 0xC0000080;
    pub const SCE: u64 = 1 << 0;
    pub const LME: u64 = 1 << 8;
    pub const LMA: u64 = 1 << 10;
}

fn en_flag(flag: u64, feature: bool) -> u64 {
    if feature {
        flag
    }
    else {
        0
    }
}

#[cfg(debug_assertions)]
fn log_registers() {
    unsafe {
        debug!("CR0={:#X}, CR4={:#X}, EFER={:#X}, RFLAGS={:#X}", asm::read_cr0(), asm::read_cr4(), asm::rdmsr(EFER::ADDRESS), asm::read_rflags());
    }
}


pub fn init() {
    let features = *features::CPU_FEATURES.get().unwrap().lock();
    unsafe {
        CPUReg::<CR0>::init(CR0::PE | CR0::ET | CR0::NE | CR0::PG | CR0::WP);
        CPUReg::<CR4>::init(CR4::PAE | en_flag(CR4::PGE, features.pge) | CR4::PCE | en_flag(CR4::UMIP, features.umip) 
        | en_flag(CR4::SMEP, features.smep) | en_flag(CR4::SMAP, features.smap));

        CPUReg::<EFER>::init(EFER::SCE | EFER::LME | EFER::LMA);
        CPUReg::<RFLAGS>::clear(RFLAGS::IOPL | RFLAGS::AC);
    }

#[cfg(debug_assertions)]
    log_registers();
}