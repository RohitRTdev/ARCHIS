#![no_std]

mod utils;
pub use utils::*;

#[repr(C)]
pub struct MemoryRegion {
    base_address: usize,
    size: usize
}


#[repr(C)]
pub struct BootInfo {
    pub kernel_desc: MemoryRegion,
    pub device_tree_desc: MemoryRegion,
    pub framebuffer_desc: MemoryRegion,
    pub memory_map_desc: MemoryRegion,
}