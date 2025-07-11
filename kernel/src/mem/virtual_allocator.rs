use crate::{mem::{fixed_allocator::{FixedAllocator, Regions::*}, PageDescriptor, KERNEL_HALF_OFFSET, KERNEL_HALF_OFFSET_RAW}, REMAP_LIST};
use crate::sync::{Once, Spinlock};
use crate::hal::{self, PageMapper};
use crate::ds::*;
use crate::error::KError;
use crate::logger::info;
use crate::{RemapEntry, RemapType::*};
use core::alloc::Layout;
use core::ptr::NonNull;
use common::{ceil_div, en_flag, PAGE_SIZE};
use super::PHY_MEM_CB;

const ERROR_MESSAGE: &'static str = "System in bad state. Critical memory failure";

pub struct VirtMemConBlk {
    total_memory: usize,
    avl_memory: usize,
    free_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>,
    alloc_block_list: List<PageDescriptor, FixedAllocator<ListNode<PageDescriptor>, {Region0 as usize}>>,
    page_mapper: PageMapper,
    proc_id: usize
}

static ADDRESS_SPACES: Once<Spinlock<List<VirtMemConBlk, FixedAllocator<ListNode<VirtMemConBlk>, {Region1 as usize}>>>> = Once::new();

// We can't use Arc or something similar here since we don't yet have heap allocation
static ACTIVE_VIRTUAL_CON_BLK: Once<Spinlock<NonNull<ListNode<VirtMemConBlk>>>> = Once::new();

impl VirtMemConBlk {
    #[cfg(target_arch="x86_64")]
    pub fn new() -> Self {
        // Since virtual address has max size of 48 bits
        // But from address 0x1ff << 39 onwards we reserve for page tables, so don't use it for conventional memory
        // We decrement one page, since we don't want page 0 in virtual address space

        let total_memory = (0x1ff << 39) - PAGE_SIZE;
        let num_pages_user = ceil_div(KERNEL_HALF_OFFSET_RAW - PAGE_SIZE, PAGE_SIZE);
        let num_pages_kernel = ceil_div((0x1ff << 39) - KERNEL_HALF_OFFSET_RAW, PAGE_SIZE);
        let mut free_block_list= List::new();
        
        // Create separate blocks for user and kernel memory
        free_block_list.add_node(PageDescriptor {
            num_pages: num_pages_user, start_phy_address: 0, start_virt_address: PAGE_SIZE, flags: 0
        }).unwrap();
        
        free_block_list.add_node(PageDescriptor {
            num_pages: num_pages_kernel, start_phy_address: 0, start_virt_address: KERNEL_HALF_OFFSET, flags: 0
        }).unwrap();

        Self {
            total_memory,
            avl_memory: total_memory,
            free_block_list,
            alloc_block_list: List::new(),
            page_mapper: PageMapper::new(),
            proc_id: 0 
        }
    }
  
    fn find_best_fit(&mut self, pages: usize, is_user: bool) -> Result<*mut u8, KError> {
        let mut smallest_blk: Option<&mut ListNode<PageDescriptor>> = None;

        // Track the block with the smallest number of pages that can satisfy above request
        // For kernel pages, make sure that allocated address is above KERNEL_HALF_OFFSET
        for block in self.free_block_list.iter_mut() {
            if block.num_pages >= pages && 
            ((is_user && block.start_virt_address < KERNEL_HALF_OFFSET) || (!is_user && block.start_virt_address >= KERNEL_HALF_OFFSET)) {
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
            let start_address = node.start_virt_address as *mut u8;

            node.start_virt_address += pages * PAGE_SIZE;
            if node.num_pages == 0 {
                let list_node = NonNull::from(node);
                unsafe {
                    self.free_block_list.remove_node(list_node);
                }
            }

            return Ok(start_address);
        }
        else {
            return Err(KError::OutOfMemory);
        }
    }

    fn coalesce_block(&mut self, addr: usize, num_pages: usize) {
        let mut found_blk = None; 
        let num_size = num_pages * PAGE_SIZE;
        
        // Check if this block can be merged with an existing block
        for blk in self.free_block_list.iter_mut() {
            // Keep kernel and user blocks separate
            if blk.start_virt_address + blk.num_pages * PAGE_SIZE == addr && addr != KERNEL_HALF_OFFSET {
                blk.num_pages += num_pages;
                found_blk = Some(NonNull::from(&*blk));
                break;
            }
            else if addr + num_size == blk.start_virt_address && blk.start_virt_address != KERNEL_HALF_OFFSET {
                blk.start_virt_address -= num_size;
                blk.num_pages += num_pages;
                found_blk = Some(NonNull::from(&*blk));
                break;
            }
        }

        // Now run same algorithm once more (There could be atmost 2 blocks to which a fragmented block could be merged)        
        if let Some(blk) = found_blk {
            let blk_desc = unsafe {blk.as_ref()};
            let merge_blk = self.free_block_list.iter_mut().find(|item| {
                (item.start_virt_address + item.num_pages * PAGE_SIZE == blk_desc.start_virt_address && blk_desc.start_virt_address != KERNEL_HALF_OFFSET) || 
                (blk_desc.start_virt_address + blk_desc.num_pages * PAGE_SIZE == item.start_virt_address && item.start_virt_address != KERNEL_HALF_OFFSET) 
            });

            // We found one more block to which the new block can be merged
            // In this case all three blocks are merged as one
            if let Some(merge_blk_desc) = merge_blk {
                merge_blk_desc.num_pages += blk_desc.num_pages;
                merge_blk_desc.start_virt_address = blk_desc.start_virt_address.min(merge_blk_desc.start_virt_address);
                unsafe {
                    self.free_block_list.remove_node(blk);
                }
            }
        } 
        else {
            // If no block to which the fragmented region can be merged, just create a new block to describe the free region
            // If it fails at this point, it's hard to recover
            self.free_block_list.add_node(PageDescriptor { num_pages, start_phy_address: 0, start_virt_address: addr as usize, flags: 0 })
            .expect(ERROR_MESSAGE);
        }
    }

    fn allocate(&mut self, layout: Layout, flags: u8) -> Result<*mut u8, KError> {
        if layout.size() >= self.avl_memory {
            return Err(KError::OutOfMemory);
        }

        if layout.align() > PAGE_SIZE || flags & PageDescriptor::VIRTUAL == 0 {
            return Err(KError::InvalidArgument);
        }

        let num_pages = ceil_div(layout.size(), PAGE_SIZE);
        let virt_addr = self.find_best_fit(num_pages, flags & PageDescriptor::USER != 0)?;    

        // The user only wants to allocate new address in virtual space
        // This is useful when the user already has some physical memory, but needs to map it to some virtual location
        if flags & PageDescriptor::NO_ALLOC != 0 {
            // Mark block as NO_ALLOC, this tells allocator that user has allocated but it's yet to map this region
            // Required so that phy-virt and virt-phy translation functions continue to work properly
            self.alloc_block_list.add_node(PageDescriptor { num_pages, start_phy_address: 0, 
                start_virt_address: virt_addr as usize, flags: PageDescriptor::NO_ALLOC})
                .expect(ERROR_MESSAGE);
            
            return Ok(virt_addr);
        }
        // Now we have got virtual address, get physical memory
        let phys_addr = PHY_MEM_CB.get().unwrap().lock().allocate(layout)?;
        // Current design choice is such that page_mapper should not fail (Kernel panics if it does)
        #[cfg(not(test))]
        self.page_mapper.map_memory(virt_addr as usize, phys_addr as usize, num_pages * PAGE_SIZE, flags);

        self.alloc_block_list.add_node(PageDescriptor { num_pages, start_phy_address: phys_addr as usize, 
            start_virt_address: virt_addr as usize, flags})
            .expect(ERROR_MESSAGE);

        Ok(virt_addr)
    }

    fn deallocate(&mut self, addr: *mut u8, layout: Layout) -> Result<(), KError> {
        if layout.align() > PAGE_SIZE {
            return Err(KError::InvalidArgument);
        }

        let num_pages = ceil_div(layout.size(), PAGE_SIZE);
        let num_size = num_pages * PAGE_SIZE;

        // Remove node from alloc_block_list
        let mut alloc_blk = None;
        for blk in self.alloc_block_list.iter() {
            if blk.start_virt_address == addr as usize && blk.num_pages == num_pages {
                alloc_blk = Some(NonNull::from(blk));

                // It is required for the memory being deallocated to have been mapped to physical memory
                debug_assert!(blk.flags & PageDescriptor::VIRTUAL != 0);
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

        #[cfg(not(test))]
        self.page_mapper.unmap_memory(addr as usize, num_size);
        
        self.coalesce_block(addr as usize, num_pages);

        Ok(())
    }

    fn get_phys_address(&mut self, virt_addr: usize) -> Option<usize> {
        // Check all locations linearly to get the physical address
        for blk in self.alloc_block_list.iter() {
            if blk.start_virt_address >= virt_addr && blk.start_virt_address + blk.num_pages * PAGE_SIZE < virt_addr
            && blk.flags & PageDescriptor::VIRTUAL != 0 {
                return Some(blk.start_phy_address + virt_addr - blk.start_virt_address);
            }
        }

        None
    }

    fn get_virt_address(&mut self, phys_addr: usize) -> Option<usize> {
        // Check all locations linearly to get the virtual address
        for blk in self.alloc_block_list.iter() {
            if blk.start_phy_address >= phys_addr && blk.start_phy_address + blk.num_pages * PAGE_SIZE < phys_addr 
            && blk.flags & PageDescriptor::VIRTUAL != 0 {
                return Some(hal::canonicalize_virtual(blk.start_virt_address + phys_addr - blk.start_phy_address));
            }
        }

        None
    }

    fn map_memory(&mut self, phys_addr: usize, virt_addr: usize, size: usize, is_user: bool) -> Result<(), KError> {
        
        // We want the addresses to be aligned to PAGE_SIZE
        #[cfg(target_arch="x86_64")]
        if phys_addr & (PAGE_SIZE - 1) != 0 || virt_addr & (PAGE_SIZE - 1) != 0 {
            return Err(KError::InvalidArgument);
        }
        
        let blk = self.alloc_block_list.iter().find(|item| {
            virt_addr >= item.start_virt_address 
            && virt_addr < item.start_virt_address + item.num_pages * PAGE_SIZE 
            && virt_addr + size <= item.start_virt_address + item.num_pages * PAGE_SIZE
        });

        let flags = en_flag!(is_user, PageDescriptor::USER) | PageDescriptor::VIRTUAL; 

        if let Some(desc) = blk {
            let top = PageDescriptor {
                num_pages: ceil_div(virt_addr - desc.start_virt_address, PAGE_SIZE),
                start_phy_address: 0,
                start_virt_address: desc.start_virt_address,
                flags: 0 
            };

            let middle = PageDescriptor {
                num_pages: ceil_div(size, PAGE_SIZE),
                start_phy_address: phys_addr,
                start_virt_address: virt_addr,
                flags 
            };

            let bottom = PageDescriptor {
                num_pages: ceil_div(desc.num_pages * PAGE_SIZE  - ((virt_addr + size) - desc.start_virt_address), PAGE_SIZE),
                start_phy_address: 0,
                start_virt_address: virt_addr + size,
                flags: 0
            };
            

            unsafe {
                self.alloc_block_list.remove_node(NonNull::from(desc));
            }

            for descriptor in [top, middle, bottom] {
                if descriptor.num_pages != 0 {
                    self.alloc_block_list.add_node(descriptor).expect(ERROR_MESSAGE);
                }
            }

        }
        else {

            // In this case, try and allocate this virtual address first 
            // If the region in question straddles both kernel and user region, this check will definitely fail
            // For now, we are going with assumption that identity mapped memory region will be entirely present in user half
            // as kernel half of memory is at a very high address for it to be mapped as conventional DRAM memory. 
            let free_blk = self.free_block_list.iter().find(|item| {
                virt_addr >= item.start_virt_address 
                && virt_addr < item.start_virt_address + item.num_pages * PAGE_SIZE 
                && virt_addr + size <= item.start_virt_address + item.num_pages * PAGE_SIZE
            });

            if let Some(desc) = free_blk {
                let top = PageDescriptor {
                    num_pages: ceil_div(virt_addr - desc.start_virt_address, PAGE_SIZE),
                    start_phy_address: 0,
                    start_virt_address: desc.start_virt_address,
                    flags: 0 
                };

                let middle = PageDescriptor {
                    num_pages: ceil_div(size, PAGE_SIZE),
                    start_phy_address: phys_addr,
                    start_virt_address: virt_addr,
                    flags 
                };

                let bottom = PageDescriptor {
                    num_pages: ceil_div(desc.num_pages * PAGE_SIZE  - ((virt_addr + size) - desc.start_virt_address), PAGE_SIZE),
                    start_phy_address: 0,
                    start_virt_address: virt_addr + size,
                    flags: 0
                };
                
                unsafe {
                    self.free_block_list.remove_node(NonNull::from(desc));
                }

                for descriptor in [top, bottom] {
                    if descriptor.num_pages != 0 {
                        self.free_block_list.add_node(descriptor).expect(ERROR_MESSAGE);
                    }
                }
                
                self.alloc_block_list.add_node(middle).expect(ERROR_MESSAGE);
            }
            else {
                info!("map_memory could not reserve memory of size:{} at address:{:#X}", size, virt_addr);
                return Err(KError::OutOfMemory);
            }
        }
        
        self.page_mapper.map_memory(virt_addr, phys_addr, size, flags);
        Ok(())
    }

    fn unmap_memory(&mut self, virt_addr: *mut u8, size: usize) -> Result<(), KError> {
        let num_pages = ceil_div(size, PAGE_SIZE);
        let blk = self.alloc_block_list.iter().find(|item| {
            item.start_virt_address == virt_addr as usize && item.num_pages == num_pages 
        });
        
        if let Some(desc) = blk {
            unsafe {
                self.alloc_block_list.remove_node(NonNull::from(desc));
            }
        }
        else {
            // In case caller tries to free memory which has not been allocated, then we return here
            return Err(KError::InvalidArgument);
        } 
        
        self.page_mapper.unmap_memory(virt_addr as usize, size);
        self.coalesce_block(virt_addr as usize, num_pages);
    
        Ok(())
    }
}

// Central API to call into both physical and virtual allocator
pub fn allocate_memory(layout: Layout, flags: u8) -> Result<*mut u8, KError> {
    if (flags & PageDescriptor::VIRTUAL != 0) && ACTIVE_VIRTUAL_CON_BLK.is_init() {
        let allocator = ACTIVE_VIRTUAL_CON_BLK.get().unwrap().lock();
        unsafe {
            (*allocator.as_ptr()).allocate(layout, flags)
        }
    }
    else {
        // Perform only physical allocation
        PHY_MEM_CB.get().unwrap().lock().allocate(layout)
    }
}

pub fn deallocate_memory(addr: *mut u8, layout: Layout, flags: u8) -> Result<(), KError> {
    if (flags & PageDescriptor::VIRTUAL != 0) && ACTIVE_VIRTUAL_CON_BLK.is_init() {
        let allocator = ACTIVE_VIRTUAL_CON_BLK.get().unwrap().lock();
        unsafe {
            (*allocator.as_ptr()).deallocate(addr, layout)
        }
    }
    else {
        PHY_MEM_CB.get().unwrap().lock().deallocate(addr, layout)
    }
}

pub fn get_physical_address(virt_addr: usize) -> Option<usize> {
    if ACTIVE_VIRTUAL_CON_BLK.is_init() {
        unsafe {
            (*ACTIVE_VIRTUAL_CON_BLK.get().unwrap().lock().as_ptr()).get_phys_address(virt_addr)
        }
    }
    else {
        // Since virtual_mem = physical_mem
        Some(virt_addr)
    }
}

pub fn get_virtual_address(phys_addr: usize) -> Option<usize> {
    if ACTIVE_VIRTUAL_CON_BLK.is_init() {
        unsafe {
            (*ACTIVE_VIRTUAL_CON_BLK.get().unwrap().lock().as_ptr()).get_virt_address(phys_addr)
        }
    }
    else {
        // Since virtual_mem = physical_mem
        Some(hal::canonicalize_virtual(phys_addr))
    }
}

pub fn virtual_allocator_init() {
    // Create the kernel address space and attach it to first node in address space list
    let remap_list = REMAP_LIST.lock();

    let mut kernel_addr_space = VirtMemConBlk::new();
    // First map the identity mapped regions
    // In case, identity mapped region straddles the kernel upper half, the checks within function will halt kernel
    // We can take it up later
    remap_list.iter().filter(|item| {
        item.map_type == IdentityMapped
    }).for_each(|item| {
        info!("Identity mapping region of size:{} with physical address:{:#X}", 
        item.value.size, item.value.base_address);
        kernel_addr_space.map_memory(
            item.value.base_address, item.value.base_address, 
            item.value.size, false).unwrap();
    });

    // Now map remaining set of regions onto upper half of memory
    remap_list.iter().filter(|item| {
        item.map_type != IdentityMapped
    }).for_each(|item| {
        let layout = Layout::from_size_align(item.value.size, PAGE_SIZE).unwrap();
        let virt_addr = kernel_addr_space.allocate(layout, PageDescriptor::VIRTUAL | PageDescriptor::NO_ALLOC)
        .expect("System could not find suitable memory in higher half kernel space") as usize;
        
        info!("Mapping region of size:{} with physical address:{:#X} to virtual address:{:#X}", 
        item.value.size, item.value.base_address, virt_addr);

        kernel_addr_space.map_memory(item.value.base_address, virt_addr, item.value.size, false).unwrap();
        
        // Update user of new location
        if let OffsetMapped(f) = &item.map_type {
            f(virt_addr);
        }
    });

    ADDRESS_SPACES.call_once(|| {
        let mut l = List::new();
        l.add_node(kernel_addr_space).unwrap();

        Spinlock::new(l)
    }); 

    ACTIVE_VIRTUAL_CON_BLK.call_once(|| {
        Spinlock::new(NonNull::from(ADDRESS_SPACES.get().unwrap().lock().first().unwrap()))
    });    
}


#[cfg(test)]
pub fn virtual_allocator_test() {
    let mut allocator = VirtMemConBlk::new();

    // Check allocating from user memory
    let layout = Layout::from_size_align(10 * PAGE_SIZE, 4096).unwrap();
    let ptr = allocator.allocate(layout, PageDescriptor::VIRTUAL | PageDescriptor::USER).unwrap();

    assert_eq!(ptr as usize, 4096);
    assert!(allocator.free_block_list.get_nodes() == 2 && allocator.free_block_list.first().unwrap().start_virt_address == 11 * PAGE_SIZE);

    let ptr1 = allocator.allocate(layout, PageDescriptor::VIRTUAL | PageDescriptor::USER).unwrap();
    assert_eq!(ptr1 as usize, 11 * PAGE_SIZE);

    let ptr2: *mut u8 = allocator.allocate(layout, PageDescriptor::VIRTUAL | PageDescriptor::USER).unwrap();
    assert_eq!(ptr2 as usize, 21 * PAGE_SIZE);

    allocator.deallocate(ptr1, layout).unwrap();
    assert_eq!(allocator.free_block_list.get_nodes(), 3);    
    let nodes = [31 * PAGE_SIZE, KERNEL_HALF_OFFSET, 11 * common::PAGE_SIZE];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });

    // Check coalescing
    allocator.deallocate(ptr, layout).unwrap();
    assert_eq!(allocator.free_block_list.get_nodes(), 3);
    let nodes = [31 * PAGE_SIZE, KERNEL_HALF_OFFSET, common::PAGE_SIZE];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });

    assert!(allocator.deallocate(ptr1, layout).is_err_and(|e| {
        e == KError::InvalidArgument
    }));

    allocator.deallocate(ptr2, layout).unwrap();
    assert_eq!(allocator.free_block_list.get_nodes(), 2);
    
    let nodes = [KERNEL_HALF_OFFSET, PAGE_SIZE];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });

    // Try allocating from kernel memory and checking
    let ptr = allocator.allocate(layout, PageDescriptor::VIRTUAL).unwrap();
    assert_eq!(ptr as usize, KERNEL_HALF_OFFSET);

    let ptr1 = allocator.allocate(layout, PageDescriptor::VIRTUAL).unwrap();
    assert_eq!(ptr1 as usize, KERNEL_HALF_OFFSET + 10 * PAGE_SIZE);
    assert_eq!(allocator.free_block_list.get_nodes(), 2);
    
    let nodes = [KERNEL_HALF_OFFSET + 20 * PAGE_SIZE, common::PAGE_SIZE];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });

    allocator.deallocate(ptr, layout).unwrap();
    assert_eq!(allocator.free_block_list.get_nodes(), 3);
    let nodes = [KERNEL_HALF_OFFSET + 20 * PAGE_SIZE, common::PAGE_SIZE, KERNEL_HALF_OFFSET];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });

    // Back to square 1
    allocator.deallocate(ptr1, layout).unwrap();
    assert_eq!(allocator.free_block_list.get_nodes(), 2);
    let nodes = [PAGE_SIZE, KERNEL_HALF_OFFSET];
    allocator.free_block_list.iter().zip(nodes).for_each(|(blk, address)| {
        assert_eq!(blk.start_virt_address, address);
    });
}