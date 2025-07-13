use crate::sync::{Once, Spinlock};
use crate::{debug, info};
use super::asm;

enum FeatureState {
    Required(&'static str),
    NotRequired(fn(&mut CPUFeatures))
}


struct FeatureDescriptor {
    fn_num: u32,
    ext_fn_num: u32,
    reg_idx: u8,
    bit_idx: u8,
    is_required: FeatureState,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CPUFeatures {
    pub umip: bool,
    pub smep: bool,
    pub smap: bool,
    pub pge: bool,
    pub mtrr: bool,

    pub phy_addr_width: u8
}

const FEATURE_MAP: [FeatureDescriptor; 9] = [
    FeatureDescriptor {
        fn_num: 0x1,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 6,
        is_required: FeatureState::Required("PAE")
    },
    FeatureDescriptor {
        fn_num: 0x80000001,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 11,
        is_required: FeatureState::Required("Syscall/Sysret")
    },
    FeatureDescriptor {
        fn_num: 0x1,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 5,
        is_required: FeatureState::Required("MSR")
    },
    FeatureDescriptor {
        fn_num: 0x1,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 9,
        is_required: FeatureState::Required("APIC")
    },
    FeatureDescriptor {
        fn_num: 0x7,
        ext_fn_num: 0,
        reg_idx: 1,
        bit_idx: 7,
        is_required: FeatureState::NotRequired(|val| {
            val.smep = true;
        })
    },
    FeatureDescriptor {
        fn_num: 0x7,
        ext_fn_num: 0,
        reg_idx: 1,
        bit_idx: 20,
        is_required: FeatureState::NotRequired(|val| {
            val.smap = true;
        })
    },
    FeatureDescriptor {
        fn_num: 0x7,
        ext_fn_num: 0,
        reg_idx: 2,
        bit_idx: 2,
        is_required: FeatureState::NotRequired(|val| {
            val.umip = true;
        })
    },
    FeatureDescriptor {
        fn_num: 0x1,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 13,
        is_required: FeatureState::NotRequired(|val| {
            val.pge = true;
        })
    },
    FeatureDescriptor {
        fn_num: 0x1,
        ext_fn_num: 0,
        reg_idx: 3,
        bit_idx: 12,
        is_required: FeatureState::NotRequired(|val| {
            val.mtrr = true;
        })
    }
];


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
    CPU_FEATURES.call_once(|| {
        let mut inst = CPUFeatures::default();

        for desc in &FEATURE_MAP {
            let val = check_bit(desc.bit_idx as u32, cpuid(desc.fn_num, desc.ext_fn_num)[desc.reg_idx as usize]);
            match desc.is_required {
                FeatureState::Required(err_str) => {
                    if !val {
                        panic!("Aris requires {} feature for intel/amd cpus", err_str);
                    }
                },
                FeatureState::NotRequired(f) => {
                    if val {
                        f(&mut inst);
                    }
                }
            }
        }

        inst.phy_addr_width = (cpuid(0x80000008,0)[0] & 0xff) as u8;
        info!("CPU max physical address width = {}", inst.phy_addr_width);
        Spinlock::new(
            inst
        )
    });


    debug!("Features = {:?}", *CPU_FEATURES.get().unwrap().lock());
}