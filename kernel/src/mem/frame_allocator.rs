use common::{MemType, MemoryDesc, PAGE_SIZE};
use crate::{ds::*, RemapEntry, RemapType::*, BOOT_INFO, REMAP_LIST};
use crate::sync::{Once, Spinlock};
use crate::error::KError;
use crate::{info, debug};
use super::{FixedAllocator, Regions::*};
use super::PageDescriptor;
use core::alloc::Layout;
use core::ptr::NonNull;


pub struct PhyMemConBlk {
    total_memory: usize,
    avl_memory: usize,
    free_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>,
    alloc_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>, 
}

pub static PHY_MEM_CB: Once<Spinlock<PhyMemConBlk>> = Once::new();

impl PhyMemConBlk {
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
            node.start_phy_address += pages * PAGE_SIZE;
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

    pub fn allocate(&mut self, layout: Layout) -> Result<*mut u8, KError> {
        if layout.size() >= self.avl_memory {
            return Err(KError::OutOfMemory);
        }

        if layout.align() > PAGE_SIZE {
            return Err(KError::InvalidArgument);
        }

        let num_pages = common::ceil_div(layout.size(), PAGE_SIZE);
        let addr = self.find_best_fit(num_pages)?;    

        Ok(addr)
    }

    pub fn deallocate(&mut self, addr: *mut u8, layout: Layout) -> Result<(), KError> {
        if layout.align() > PAGE_SIZE {
            return Err(KError::InvalidArgument);
        }

        let num_pages = common::ceil_div(layout.size(), PAGE_SIZE);

        // Remove node from alloc_block_list
        let mut alloc_blk = None;
        for blk in self.alloc_block_list.iter() {
            if blk.start_phy_address == addr as usize && blk.num_pages == num_pages {
                alloc_blk = Some(NonNull::from(blk));
                break;
            }
        }
        
        if let Some(blk) = alloc_blk {
            unsafe {
                self.alloc_block_list.remove_node(blk);
            }
        }
        else {
            // In case caller tries to free memory which has not been allocated, then we return here
            return Err(KError::InvalidArgument);
        } 
        
        let mut found_blk = None; 
        let num_size = num_pages * PAGE_SIZE;
        let addr = addr as usize;
        
        // Check if this block can be merged with an existing block
        for blk in self.free_block_list.iter_mut() {
            if blk.start_phy_address + blk.num_pages * PAGE_SIZE == addr {
                blk.num_pages += num_pages;
                found_blk = Some(NonNull::from(&*blk));
                break;
            }
            else if addr + num_size == blk.start_phy_address {
                blk.start_phy_address -= num_size;
                blk.num_pages += num_pages;
                found_blk = Some(NonNull::from(&*blk));
                break;
            }
        }

        // Now run same algorithm once more (There could be atmost 2 blocks to which a fragmented block could be merged)        
        if let Some(blk) = found_blk {
            let blk_desc = unsafe {blk.as_ref()};
            let merge_blk = self.free_block_list.iter_mut().find(|item| {
                (item.start_phy_address + item.num_pages * PAGE_SIZE == blk_desc.start_phy_address) || 
                (blk_desc.start_phy_address + blk_desc.num_pages * PAGE_SIZE == item.start_phy_address) 
            });

            // We found one more block to which the new block can be merged
            // In this case all three blocks are merged as one
            if let Some(merge_blk_desc) = merge_blk {
                merge_blk_desc.num_pages += blk_desc.num_pages;
                merge_blk_desc.start_phy_address = blk_desc.start_phy_address.min(merge_blk_desc.start_phy_address);
                unsafe {
                    self.free_block_list.remove_node(blk);
                }
            }
        } 
        else {
            // If no block to which the fragmented region can be merged, just create a new block to describe the free region
            // If it fails at this point, it's hard to recover
            self.free_block_list.add_node(PageDescriptor { num_pages, start_phy_address: addr, start_virt_address: 0, flags: 0 })
            .expect("System in bad state. Critical memory failure!");
        }
        Ok(())
    }
}


pub fn frame_allocator_init() {
    let boot_info = BOOT_INFO.get().unwrap().lock();
    let mut init_mem_cb = PhyMemConBlk {
        total_memory: 0,
        avl_memory: 0,
        free_block_list: List::new(),
        alloc_block_list: List::new()
    };

    let mem_descriptors  = unsafe {
        core::slice::from_raw_parts_mut(boot_info.memory_map_desc.start as *mut MemoryDesc, boot_info.memory_map_desc.size / boot_info.memory_map_desc.entry_size)
    };

    for desc in mem_descriptors {
        // Remove page 0 from frame allocation. Since various systems consider 0 as null value,
        // we will not include it
        if desc.val.base_address == 0 {
            desc.val.base_address += PAGE_SIZE;
            if desc.val.size > PAGE_SIZE {
                desc.val.size -= PAGE_SIZE;
            }
            else {
                continue;
            }
        }

        match &desc.mem_type {
            MemType::Free => {
                init_mem_cb.free_block_list.add_node(PageDescriptor { num_pages: common::ceil_div(desc.val.size, PAGE_SIZE), 
                    start_phy_address: desc.val.base_address, start_virt_address: 0, flags: 0 }).unwrap();
                
                init_mem_cb.avl_memory += desc.val.size;
            },
            MemType::Allocated | MemType::Runtime => {
                init_mem_cb.alloc_block_list.add_node(PageDescriptor { num_pages: common::ceil_div(desc.val.size, PAGE_SIZE), 
                    start_phy_address: desc.val.base_address, start_virt_address: 0, flags: 0 }).unwrap();
            
                    if desc.mem_type == MemType::Runtime {
                        REMAP_LIST.lock().add_node(RemapEntry { 
                            value: desc.val,
                            map_type: IdentityMapped }).unwrap();
                    }
            }
        }
        init_mem_cb.total_memory += desc.val.size;
    }

    info!("Initialized Memory control block -> Total memory: {}, Available memory: {}", init_mem_cb.total_memory, init_mem_cb.avl_memory);

    PHY_MEM_CB.call_once(|| {
        Spinlock::new(init_mem_cb)
    });
}


#[cfg(test)] 
pub fn test_init_allocator() {
    let desc1 = PageDescriptor {
        num_pages: 10,
        start_phy_address: 0x0,
        start_virt_address: 0x0,
        flags: 0x0
    };

    let desc2 = PageDescriptor {
        num_pages: 2,
        start_phy_address: 20 * PAGE_SIZE,
        start_virt_address: 0x0,
        flags: 0x0
    };

    let desc3 = PageDescriptor {
        num_pages: 6,
        start_phy_address: 40 * PAGE_SIZE,
        start_virt_address: 0x0,
        flags: 0x0
    };
    
    let mut free_block_list= List::new();
    free_block_list.add_node(desc1).unwrap();
    free_block_list.add_node(desc2).unwrap();
    free_block_list.add_node(desc3).unwrap();

    let cb = PhyMemConBlk {
        total_memory: 18 * PAGE_SIZE,
        avl_memory: 18 * PAGE_SIZE,
        free_block_list,
        alloc_block_list: List::new()
    };

    if let Some(val) = PHY_MEM_CB.get() {
        *val.lock() = cb;
    }
    else {
        PHY_MEM_CB.call_once(|| {
            Spinlock::new(cb)
        });
    }
}

#[cfg(test)]
pub fn test_init_allocator_for_virtual() {
    let desc = PageDescriptor {
        num_pages: 100,
        start_phy_address: 0xf000,
        start_virt_address: 0x0,
        flags: 0x0
    };

    let mut free_block_list= List::new();
    free_block_list.add_node(desc).unwrap();

    let cb = PhyMemConBlk {
        total_memory: 100 * PAGE_SIZE,
        avl_memory: 100 * PAGE_SIZE,
        free_block_list,
        alloc_block_list: List::new()
    };

    if let Some(val) = PHY_MEM_CB.get() {
        *val.lock() = cb;
    }
    else {
        PHY_MEM_CB.call_once(|| {
            Spinlock::new(cb)
        });
    }
}

#[cfg(test)]
pub fn check_mem_nodes() {

    // We should have (8) - (2 + 6 + 2) layout
    let allocator = PHY_MEM_CB.get().unwrap().lock();

    assert_eq!(allocator.free_block_list.get_nodes(), 1);
    assert_eq!(allocator.alloc_block_list.get_nodes(), 3);

    let free_list = [8];
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
