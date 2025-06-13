#![no_std]

//extern crate alloc;
mod utils;
pub use utils::*;
pub mod elf;
//use alloc::{collections::BTreeMap, string::String, vec::Vec};

#[cfg(target_arch="x86_64")]
pub const PAGE_SIZE: usize = 4096;
pub const MAX_DESCRIPTORS: usize = 50;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ArrayTable {
    pub start: usize,
    pub size: usize,
    pub entry_size: usize
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ModuleInfo {
    pub entry: usize,
    pub base: usize,
    pub size: usize,
    pub sym_tab: Option<ArrayTable>,
    pub sym_str: Option<MemoryRegion>,
    pub dyn_tab: Option<ArrayTable>,
    pub dyn_str: Option<MemoryRegion>,
    pub rlc_shn: Option<ArrayTable>,
    pub dyn_shn: Option<ArrayTable>
}

#[repr(C)]
pub enum MemType {
    Free,
    Allocated,
    Runtime
}

#[repr(C)]
pub struct MemoryDesc {
    pub val: MemoryRegion,
    pub mem_type: MemType
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub base_address: usize,
    pub size: usize
}


#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BootInfo {
    pub kernel_desc: ModuleInfo,
    pub framebuffer_desc: FBInfo,
    pub memory_map_desc: ArrayTable,
 //   pub init_fs: BTreeMap<String, Vec<u8>>
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct PixelMask {
    pub red_mask: u32,
    pub blue_mask: u32,
    pub green_mask: u32,
    pub alpha_mask: u32
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FBInfo {
    pub fb: MemoryRegion,
    pub height: usize,
    pub width: usize,
    pub stride: usize,
    pub pixel_mask: PixelMask

}