use core::alloc::Layout;
use core::ffi::c_void;

pub mod fixed_allocator;
use fixed_allocator::*;
pub trait Allocator {
    fn alloc(layout: Layout) -> *mut c_void;
    fn dealloc(address: *mut c_void); 
}