use core::alloc::Layout;
use core::ptr::NonNull;
use crate::error::KError;

mod fixed_allocator;
mod physical_allocator;
pub use fixed_allocator::*;
pub use physical_allocator::*;


pub trait Allocator<T> {
    fn alloc(layout: Layout) -> Result<NonNull<T>, KError>;
    unsafe fn dealloc(address: NonNull<T>, layout: Layout); 
}