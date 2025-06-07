#![no_std]

mod utils;
pub use utils::*;
pub mod elf;

#[repr(C)]
#[derive(Debug)]
pub struct SymTable {
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
    pub sym_tab: Option<SymTable>,
    pub reloc_section: Option<SymTable>,
    pub dynamic_section: Option<SymTable>
}


#[repr(C)]
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