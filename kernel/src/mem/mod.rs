use core::alloc::Layout;
use core::ptr::NonNull;
use crate::error::KError;

mod fixed_allocator;
mod frame_allocator;
mod virtual_allocator;
mod heap_allocator;
pub use fixed_allocator::*;
pub use frame_allocator::*;
pub use virtual_allocator::*;

// This is in canonical form
#[cfg(target_arch="x86_64")]
pub const KERNEL_HALF_OFFSET: usize = 0xffff800000000000; 
const KERNEL_HALF_OFFSET_RAW: usize = 0x0000800000000000; 

pub trait Allocator<T> {
    fn alloc(layout: Layout) -> Result<NonNull<T>, KError>;
    unsafe fn dealloc(address: NonNull<T>, layout: Layout); 
}

#[derive(Debug)]
pub struct PageDescriptor {
    num_pages: usize,
    start_phy_address: usize,
    start_virt_address: usize,
    flags: u8
}


impl PageDescriptor {
    pub const VIRTUAL: u8 = 1;
    pub const USER: u8 = 1 << 1;
    pub const NO_ALLOC: u8 = 1 << 2;
}

pub fn init() {
    fixed_allocator_init();
    frame_allocator_init();
    virtual_allocator_init();
}