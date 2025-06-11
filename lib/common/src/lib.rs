#![no_std]

mod utils;
pub use utils::*;
pub mod elf;

#[repr(C)]
#[derive(Debug)]
pub struct ArrayTable {
    pub start: usize,
    pub size: usize,
    pub entry_size: usize
}

#[repr(C)]
#[derive(Debug)]
pub struct KernelInfo {
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
#[derive(Debug)]
pub struct MemoryRegion {
    pub base_address: usize,
    pub size: usize
}


#[repr(C)]
pub struct BootInfo {
    pub kernel_desc: KernelInfo,
    pub device_tree_desc: MemoryRegion,
    pub framebuffer_desc: MemoryRegion,
    pub memory_map_desc: MemoryRegion,
}