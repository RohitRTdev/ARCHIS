use core::alloc::Layout;
use core::ptr::NonNull;

use common::MemoryDesc;
use common::MemType;
use common::PAGE_SIZE;
use crate::{ds::*, BOOT_INFO};
use crate::sync::{Once, Spinlock};
use crate::error::KError;
use crate::logger::{info, debug};
use super::{FixedAllocator, Regions::*};

#[derive(Debug)]
struct PageDescriptor {
    num_pages: usize,
    start_phy_address: usize,
    start_virt_address: usize,
    flags: u8
}

struct PhysMemConBlk {
    total_memory: usize,
    avl_memory: usize,
    free_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>,
    alloc_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>, 
}

static PHY_MEM_CB: Once<Spinlock<PhysMemConBlk>> = Once::new();

impl PhysMemConBlk {
    fn find_best_fit(&mut self, pages: usize) -> Result<*mut u8, KError> {
        let mut smallest_blk: Option<&mut ListNode<PageDescriptor>> = None;

        // Track the block with the smallest number of pages that can satisfy above request
        for block in self.free_block_list.iter_mut() {
            if block.num_pages >= pages {
                if let Some(val) = &smallest_blk {
                    if block.num_pages < val.num_pages {
                        smallest_blk = Some(block);
                    }
                }
                else {
                    smallest_blk = Some(block);
                }
            }
        }

        if let Some(node) = smallest_blk {
            node.num_pages -= pages;
            let start_address = node.start_phy_address as *mut u8;
            node.start_phy_address += pages * common::PAGE_SIZE;
            if node.num_pages == 0 {
                let list_node = NonNull::from(node);
                unsafe {
                    self.free_block_list.remove_node(list_node);
                }
            }

            self.alloc_block_list.add_node(PageDescriptor { num_pages: pages, start_phy_address: start_address as usize, 
                start_virt_address: 0x0, flags: 0x0 })?;

            return Ok(start_address);
        }
        else {
            return Err(KError::OutOfMemory);
        }
    }

}


pub fn allocate_memory(layout: Layout, flags: u8) -> Result<*mut u8, KError> {
    let mut allocator = PHY_MEM_CB.get().unwrap().lock();

    if layout.size() >= allocator.avl_memory {
        return Err(KError::OutOfMemory);
    }

    if layout.align() > common::PAGE_SIZE {
        return Err(KError::InvalidArgument);
    }

    let num_pages = common::ceil_div(layout.size(), common::PAGE_SIZE);
    let addr = allocator.find_best_fit(num_pages)?;    

    Ok(addr)
}


pub fn deallocate_memory(addr: *mut u8, layout: Layout, flags: u8) -> Result<(), KError> {
    let mut allocator = PHY_MEM_CB.get().unwrap().lock();

    if layout.align() > common::PAGE_SIZE {
        return Err(KError::InvalidArgument);
    }

    let num_pages = common::ceil_div(layout.size(), common::PAGE_SIZE);
    let num_size = num_pages * common::PAGE_SIZE;
    let mut found_blk = false;

    // Remove node from alloc_block_list
    let mut alloc_blk = None;
    for blk in allocator.alloc_block_list.iter() {
        if blk.start_phy_address == addr as usize && blk.num_pages == num_pages {
            alloc_blk = Some(NonNull::from(blk));
            break;
        }
    }
    
    if let Some(blk) = alloc_blk {
        unsafe {
            allocator.alloc_block_list.remove_node(blk);
        }
    }
    else {
        // In case caller tries to free memory which has not been allocated, then we return here
        return Err(KError::InvalidArgument);
    } 
    
    // Check if this block can be coaleasced with an existing block
    for blk in allocator.free_block_list.iter_mut() {
        if blk.start_phy_address + blk.num_pages * common::PAGE_SIZE == addr as usize {
            blk.num_pages += num_pages;
            found_blk = true;
            break;
        }
        else if unsafe {addr.add(num_size)} as usize == blk.start_phy_address {
            blk.start_phy_address -= num_size;
            blk.num_pages += num_pages;
            found_blk = true;
            break;
        }
    }


    if !found_blk {
        allocator.free_block_list.add_node(PageDescriptor { num_pages, start_phy_address: addr as usize, start_virt_address: 0, flags: 0 })?;
    }

    Ok(())
}

pub fn init() {
    let boot_info = BOOT_INFO.get().unwrap().lock();
    let mut init_mem_cb = PhysMemConBlk {
        total_memory: 0,
        avl_memory: 0,
        free_block_list: List::new(),
        alloc_block_list: List::new()
    };

    let mem_descriptors  = unsafe {
        core::slice::from_raw_parts(boot_info.memory_map_desc.start as *const MemoryDesc, boot_info.memory_map_desc.size / boot_info.memory_map_desc.entry_size)
    };

    for desc in mem_descriptors {
        match &desc.mem_type {
            MemType::Free => {
                init_mem_cb.free_block_list.add_node(PageDescriptor { num_pages: common::ceil_div(desc.val.size, PAGE_SIZE), 
                    start_phy_address: desc.val.base_address, start_virt_address: 0, flags: 0 }).unwrap();
                
                init_mem_cb.avl_memory += desc.val.size;
            },
            MemType::Allocated | MemType::Runtime => {
                init_mem_cb.alloc_block_list.add_node(PageDescriptor { num_pages: common::ceil_div(desc.val.size, PAGE_SIZE), 
                    start_phy_address: desc.val.base_address, start_virt_address: 0, flags: 0 }).unwrap();
            }
        }
        init_mem_cb.total_memory += desc.val.size;
    }

    info!("Initialized Memory control block -> Total memory: {}, Available memory: {}", init_mem_cb.total_memory, init_mem_cb.avl_memory);
    
    debug!("Printing free block list..");
    debug!("{:?}", init_mem_cb.free_block_list);
    
    debug!("Printing alloc block list..");
    debug!("{:?}", init_mem_cb.alloc_block_list);
    
    PHY_MEM_CB.call_once(|| {
        Spinlock::new(init_mem_cb)
    });
}


#[cfg(test)] 
pub fn test_init_allocator() {
    PHY_MEM_CB.call_once(|| {
        let desc1 = PageDescriptor {
            num_pages: 10,
            start_phy_address: 0x0,
            start_virt_address: 0x0,
            flags: 0x0
        };

        let desc2 = PageDescriptor {
            num_pages: 2,
            start_phy_address: 0x10,
            start_virt_address: 0x0,
            flags: 0x0
        };

        let desc3 = PageDescriptor {
            num_pages: 5,
            start_phy_address: 0x20,
            start_virt_address: 0x0,
            flags: 0x0
        };
        
        let mut free_block_list: List<_, FixedAllocator<_, {Region0 as usize}>> = List::new();
        free_block_list.add_node(desc1).unwrap();
        free_block_list.add_node(desc2).unwrap();
        free_block_list.add_node(desc3).unwrap();

        let cb = PhysMemConBlk {
            total_memory: 17 * common::PAGE_SIZE,
            avl_memory: 17 * common::PAGE_SIZE,
            free_block_list,
            alloc_block_list: List::new()
        };

        Spinlock::new(cb)
    });
}

#[cfg(test)]
pub fn check_mem_nodes() {

    // We should have (2 + 5) - (2 + 6 + 2) layout
    let allocator = PHY_MEM_CB.get().unwrap().lock();

    assert_eq!(allocator.free_block_list.get_nodes(), 2);
    assert_eq!(allocator.alloc_block_list.get_nodes(), 3);

    let free_list = [2, 5];
    let alloc_list = [2, 6, 2];

    common::test_log!("Printing free_block_list....");
    for (idx, blk) in allocator.free_block_list.iter().enumerate() {
        assert_eq!(free_list[idx], blk.num_pages);
        common::test_log!("{:?}", **blk);
    }
    
    common::test_log!("Printing alloc_block_list....");
    for (idx, blk) in allocator.alloc_block_list.iter().enumerate() {
        assert_eq!(alloc_list[idx], blk.num_pages);
        common::test_log!("{:?}", **blk);
    }
}