use core::alloc::Layout;
use core::ptr::NonNull;

mod fixed_allocator;
pub use fixed_allocator::*;
pub trait Allocator<T> {
    fn alloc(layout: Layout) -> NonNull<T>;
    unsafe fn dealloc(address: NonNull<T>, layout: Layout); 
}