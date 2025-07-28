use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::mem::{size_of, align_of};
use common::{align_up, PAGE_SIZE};
use crate::mem::{allocate_memory, PageDescriptor};
use crate::sync::Spinlock;
use kernel_intf::{info, debug};

pub struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

pub struct LinkedListAllocator {
    head: *mut ListNode,
    backing_memory: usize
}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        Self {
            head: core::ptr::null_mut(),
            backing_memory: 0
        }
    }

    fn find_fit(&mut self, layout: Layout) -> Option<*mut ListNode> {
        let size = layout.size();
        let align = layout.align();
        let mut prev: *mut ListNode = core::ptr::null_mut();
        let mut current = self.head;
        while !current.is_null() {
            let node = unsafe { &mut *current };
            let addr = current as usize;
            let aligned_addr = align_up(addr, align);
            let padding = aligned_addr - addr;
            if node.size >= size + padding {
                // Remove node from list
                if !prev.is_null() {
                    unsafe { (*prev).next = node.next.take(); }
                } else {
                    self.head = node.next.take().map_or(core::ptr::null_mut(), |n| n);
                }
                return Some(current);
            }
            prev = current;
            current = node.next.as_deref_mut().map_or(core::ptr::null_mut(), |n| n as *mut _);
        }
        None
    }

    fn add_free_region(&mut self, addr: usize, size: usize) {
        let node = addr as *mut ListNode;
        unsafe {
            (*node).size = size;
            (*node).next = self.head.as_mut();
        }
        self.head = node;
    }

    // Given a ListNode pointer, layout, and allocator, split the node and return the aligned pointer.
    fn use_list_node(&mut self, node_ptr: *mut ListNode, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let node = unsafe { &mut *node_ptr };
        let addr = node_ptr as usize;
        let aligned_addr = align_up(addr, align);
        let next_aligned_addr = align_up(aligned_addr + size, align_of::<ListNode>());
        let remaining = node.size - (next_aligned_addr - addr);
        self.backing_memory -= size;
        if remaining >= size_of::<ListNode>() {
            self.add_free_region(next_aligned_addr, remaining);
        }
        
        aligned_addr as *mut u8
    }
}

unsafe impl GlobalAlloc for Spinlock<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        debug!("Requesting heap allocation -> {:?}", layout);
        let size = layout.size().max(size_of::<ListNode>());
        let align = layout.align().max(align_of::<ListNode>());
        let layout = Layout::from_size_align(size, align).unwrap();
        let mut allocator = self.lock();
        
        // If not enough memory is reserved, just skip the search and ask virtual allocator for memory
        if allocator.backing_memory >= size {
            if let Some(node_ptr) = allocator.find_fit(layout) {
                return allocator.use_list_node(node_ptr, layout);
            }
        }

        // Out of memory, request more from virtual allocator and retry
        let alloc_size = align_up(size, PAGE_SIZE);
        match allocate_memory(Layout::from_size_align(alloc_size, PAGE_SIZE).unwrap(), PageDescriptor::VIRTUAL).as_ref() {
            Ok(mem) => {
                allocator.add_free_region(*mem as usize, alloc_size);
                allocator.backing_memory += alloc_size;
                if let Some(node_ptr) = allocator.find_fit(layout) {
                    allocator.use_list_node(node_ptr, layout)
                } else {
                    info!("Heap allocator could not find a fit for allocation size:{} and alignment:{} despite adding new memory", size, align);
                    null_mut()
                }
            },
            Err(_) => {
                info!("Frame allocator has run out of memory for allocation size:{} and alignment:{}", size, align); 
                null_mut()
            }
        }
    }

    unsafe fn dealloc(&self, addr: *mut u8, layout: Layout) {
        let size = layout.size().max(size_of::<ListNode>());
        let mut allocator = self.lock();
        debug!("Requesting heap deallocation -> {:?} at address:{:#X} with memory:{}", layout, addr as usize, allocator.backing_memory);
        allocator.add_free_region(addr as usize, size);
        allocator.backing_memory += size;
    }
}

#[cfg(not(test))]
#[global_allocator]
pub static GLOBAL_ALLOCATOR: Spinlock<LinkedListAllocator> = Spinlock::new(LinkedListAllocator::new()); 