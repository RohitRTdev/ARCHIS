use core::marker::PhantomData;

use super::{asm, features};
use kernel_intf::debug;
use crate::devices::SERIAL;
use common::en_flag;

pub struct CR0;
pub struct CR4;
pub struct RFLAGS;
pub struct EFER;
pub struct MTRRCAP;
pub struct MTRRPHY;
pub struct MTRRPHYMASK;
pub struct MTRRDEF;
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

impl MTRRCAP {
    pub const ADDRESS: u32 = 0xFE;
    pub const VAR_REG_CNT_MASK: u64 = 0xff;
    pub const WC: u64 = 1 << 10;
}

impl MTRRPHY {
    pub const ADDRESS: u32 = 0x200;
}

impl MTRRPHYMASK {
    pub const ADDRESS: u32 = 0x201;
    pub const VALID: u64 = 1 << 11;
}

impl MTRRDEF {
    pub const ADDRESS: u32 = 0x2FF;
    pub const MTRR_ENABLE: u64 = 1 << 11;
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
        CPUReg::<CR4>::init(CR4::PAE | en_flag!(features.pge, CR4::PGE) | CR4::PCE | en_flag!(features.umip, CR4::UMIP) 
        | en_flag!(features.smep, CR4::SMEP) | en_flag!(features.smap, CR4::SMAP));

        CPUReg::<EFER>::init(EFER::SCE | EFER::LME | EFER::LMA);
        CPUReg::<RFLAGS>::clear(RFLAGS::IOPL | RFLAGS::AC);
    }

    //if features.mtrr {
    //    // Set video memory to WC caching type and disable remaining variable mtrr's
    //    let mtrr_cap = unsafe {
    //        asm::rdmsr(MTRRCAP::ADDRESS)
    //    };

    //    let wc_support = mtrr_cap & MTRRCAP::WC != 0;
    //    let mut var_reg_cnt = mtrr_cap & MTRRCAP::VAR_REG_CNT_MASK;
    //    let mut var_reg_offset = MTRRPHYMASK::ADDRESS;

    //    info!("Total MTRR variable counters = {}, WC support={}", var_reg_cnt, wc_support);
    //    assert!(var_reg_cnt <= 8);

    //    if wc_support && var_reg_cnt > 0 {
    //        // Find an unused mtrr
    //        while var_reg_cnt > 0 {
    //            unsafe {
    //                if asm::rdmsr(var_reg_offset) & 0x1 == 0 {
    //                    break;
    //                }
    //            }
    //            var_reg_offset += 2;
    //            var_reg_cnt -= 1;
    //        } 

    //        if var_reg_cnt != 0 {
    //            let fb_cb = BOOT_INFO.get().unwrap();
    //            let fb_size = fb_cb.framebuffer_desc.fb.size.next_power_of_two();
    //            let fb_base = common::align_down(fb_cb.framebuffer_desc.fb.base_address, fb_size);
    //            let mask = ((1 << features.phy_addr_width) - 1 - (fb_size - 1)) as u64 & !0xfff; 

    //            // Type 1 -> WC encoding + physical address
    //            let vid_mtrr_data: u64 = (fb_base as u64 & !0xfff) | 0x1;
    //            let vid_mtrr_mask_data: u64 = mask | MTRRPHYMASK::VALID;

    //            info!("Setting MTRRPHY{} with WC encoding, phy_data={:#X} and mask={:#X} -> fb_size={}, fb_base={:#X}", 8 - var_reg_cnt, vid_mtrr_data, vid_mtrr_mask_data, fb_size, fb_base); 
    //            unsafe {
    //                asm::wrmsr(var_reg_offset, vid_mtrr_data);
    //                asm::wrmsr(var_reg_offset + 1, vid_mtrr_mask_data);
    //            }
    //        }
    //        else {
    //            info!("Did not find unprogrammed variable mtrr. Skipping cache setting for frame buffer...");
    //        }
    //    }

    //    unsafe {
    //        let val = asm::rdmsr(MTRRDEF::ADDRESS);
    //        // If firmware has not setup default memory region type, then set it to WB
    //        if val & MTRRDEF::MTRR_ENABLE == 0 {
    //            asm::wrmsr(MTRRDEF::ADDRESS, val | MTRRDEF::MTRR_ENABLE | 0x6);
    //        }
    //    }
    //}
    
#[cfg(debug_assertions)]
    log_registers();
}