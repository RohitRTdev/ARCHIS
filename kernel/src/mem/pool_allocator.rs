use core::alloc::Layout;
use core::ptr::NonNull;
use core::marker::PhantomData;
use crate::mem::PageDescriptor;
use crate::sync::Spinlock;
use kernel_intf::KError;
use kernel_intf::debug;
use crate::ds::{List, ListNode};
use super::fixed_allocator::{FixedAllocator, Regions::*};
use super::allocate_memory;
use common::PAGE_SIZE;

const ALLOCATION_UNIT: usize = PAGE_SIZE * 2;  

// Represents a single pool for a specific block size.
struct Pool {
    block_size: usize,
    free_list: Option<NonNull<FreeBlock>>
}

impl Pool {
    fn new(block_size: usize) -> Self {
        Pool {
            block_size,
            free_list: None
        }
    }
}

// Linked list to track free slots.
#[repr(C)]
struct FreeBlock {
    next: Option<NonNull<FreeBlock>>,
}

impl FreeBlock {
    fn set_next(&mut self, next: Option<NonNull<FreeBlock>>) {
        self.next = next;
    }
}

// Maintains a list of pools for different block sizes.
struct PoolControlBlock {
    pools: List<Pool, FixedAllocator<ListNode<Pool>, {Region5 as usize}>>
}

impl PoolControlBlock {
    fn find_pool_mut(&mut self, block_size: usize) -> Option<&mut Pool> {
        self.pools.iter_mut().find(|pool| pool.block_size == block_size)
        .and_then(|item| {
            Some(&mut **item)
        })
    }

    fn add_pool(&mut self, block_size: usize) -> Result<&mut Pool, KError> {
        let pool = Pool::new(block_size);
        self.pools.add_node(pool).map_err(|_| {
            KError::OutOfMemory 
        })?;
        
        Ok(self.find_pool_mut(block_size).unwrap())
    }
}

static POOL_CB: Spinlock<PoolControlBlock> = Spinlock::new(PoolControlBlock {
    pools: List::new()
});

pub struct PoolAllocator<T> {
    _marker: PhantomData<T>,
}

impl<T> PoolAllocator<T> {
    // Push a range of slots as free blocks into the pool's free list
    fn push_free_blocks(pool: &mut Pool, base: *mut u8, slots: usize, block_size: usize) {
        debug!("pool_allocator -> push_free_blocks: base:{:#X}, slots:{}, block_size:{}",
            base as usize, slots, block_size);
        for i in 0..slots {
            let slot_ptr = unsafe { base.add(i * block_size) as *mut FreeBlock };
            unsafe {
                (*slot_ptr).set_next(pool.free_list);
                pool.free_list = Some(NonNull::new_unchecked(slot_ptr));
            }
        }
    }
}

impl<T> super::Allocator<T> for PoolAllocator<T> {
    fn alloc(layout: Layout) -> Result<NonNull<T>, KError> {
        // Due to our pool allocator's design, the alignment will be same as the size
        assert!(layout.size() >= size_of::<FreeBlock>() && layout.size() <= PAGE_SIZE
            && layout.align() <= layout.size() && layout.size() % layout.align() == 0
            && layout.size() == size_of::<T>());

        let block_size = size_of::<T>();
        let mut cb = POOL_CB.lock();

        // Find or create the pool for this block size
        let pool = match cb.find_pool_mut(block_size) {
            Some(pool) => pool,
            None => cb.add_pool(block_size)?,
        };

        // If free_list is not empty, pop and return
        if let Some(free_block) = pool.free_list {
            let next = unsafe { (*free_block.as_ptr()).next };
            pool.free_list = next;
            return Ok(free_block.cast());
        }

        // No free slots, allocate a new block and push all slots to free_list
        let slots_per_block = ALLOCATION_UNIT / block_size;
        let layout = Layout::from_size_align(ALLOCATION_UNIT, PAGE_SIZE).unwrap();
        let base = allocate_memory(layout, PageDescriptor::VIRTUAL)?;

        // Push all slots to free_list
        Self::push_free_blocks(pool, base, slots_per_block, block_size);

        // Pop one for this allocation
        if let Some(free_block) = pool.free_list {
            let next = unsafe { (*free_block.as_ptr()).next };
            pool.free_list = next;
            return Ok(free_block.cast());
        }

        Err(KError::OutOfMemory)
    }

    unsafe fn dealloc(ptr: NonNull<T>, layout: Layout) {
        assert!(layout.size() >= size_of::<FreeBlock>() && layout.size() <= PAGE_SIZE
            && layout.align() <= layout.size() && layout.size() % layout.align() == 0
            && layout.size() == size_of::<T>()); 

        let block_size = size_of::<T>();
        let mut cb = POOL_CB.lock();

        // Find the pool for this block size and add the released block back to head of free_list
        if let Some(pool) = cb.find_pool_mut(block_size) {
            let free_ptr = ptr.as_ptr() as *mut FreeBlock;
            (*free_ptr).set_next(pool.free_list);
            pool.free_list = Some(NonNull::new_unchecked(free_ptr));
        }
        else {
            debug_assert!(false, "pool_allocator -> dealloc called for unknown pointer :{:#X} and layout:{:?}",
            ptr.as_ptr() as usize, layout);
        }
    }
}