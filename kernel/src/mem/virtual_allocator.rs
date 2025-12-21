use crate::{REMAP_LIST, cpu::{self, PerCpu}, mem::{KERNEL_HALF_OFFSET, KERNEL_HALF_OFFSET_RAW, PageDescriptor, fixed_allocator::Regions::*}};
use crate::sync::{Once, Spinlock};
use crate::hal::{self, PageMapper};
use crate::ds::*;
use crate::cpu::MAX_CPUS;
use kernel_intf::KError;
use kernel_intf::{info, debug};
use crate::RemapType::*;
use core::alloc::Layout;
use core::ptr::NonNull;
use core::hint::likely;
use core::sync::atomic::{AtomicPtr, Ordering};
use common::{PAGE_SIZE, ceil_div, ptr_to_ref_mut};
use super::PHY_MEM_CB;

const ERROR_MESSAGE: &'static str = "System in bad state. Critical memory failure";

#[derive(PartialEq)]
pub enum MapFetchType {
    Any,
    Kernel
}

pub struct VirtMemConBlk {
    total_memory: usize,
    avl_memory: usize,
    free_block_list: FixedList<PageDescriptor, {Region0 as usize}>,
    alloc_block_list: FixedList<PageDescriptor, {Region0 as usize}>,
    page_mapper: PageMapper,
    proc_id: usize
}

static ADDRESS_SPACES: Once<Spinlock<FixedList<Spinlock<VirtMemConBlk>, {Region1 as usize}>>> = Once::new();

// We cannot use Arc or something similar since this is activated prior to heap initialization
static PER_CPU_ACTIVE_VCB: PerCpu<AtomicPtr<Spinlock<VirtMemConBlk>>> = PerCpu::new_with(
    [const {AtomicPtr::new(core::ptr::null_mut())}; MAX_CPUS]
);

impl VirtMemConBlk {
    #[cfg(target_arch="x86_64")]
    pub fn new(is_init_address_space: bool) -> Self {
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
            page_mapper: PageMapper::new(is_init_address_space),
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
            let merge_blk: Option<&mut ListNode<PageDescriptor>> = self.free_block_list.iter_mut().find(|item| {
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
        if layout.size() >= self.avl_memory || layout.size() > self.total_memory {
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

        self.avl_memory -= num_pages * PAGE_SIZE;
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
        let mut is_no_alloc = false;
        for blk in self.alloc_block_list.iter() {
            if blk.start_virt_address == addr as usize && blk.num_pages == num_pages {
                alloc_blk = Some(NonNull::from(blk));
                is_no_alloc = (blk.flags & PageDescriptor::NO_ALLOC) != 0;
                // It is required for the memory being deallocated to have been mapped to physical memory
                debug_assert!((blk.flags & PageDescriptor::VIRTUAL != 0) || is_no_alloc);
                break;
            }
        }
        if let Some(blk) = alloc_blk {
            unsafe {
                if !is_no_alloc {
                    PHY_MEM_CB.get().unwrap().lock().deallocate(blk.as_ref().start_phy_address as *mut u8, layout).expect(ERROR_MESSAGE);
                } 
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
        self.avl_memory += num_size;

        Ok(())
    }

    fn get_phys_address(&mut self, virt_addr: usize) -> Option<usize> {
        // Check all locations linearly to get the physical address
        for blk in self.alloc_block_list.iter() {
            if blk.start_virt_address <= virt_addr && blk.start_virt_address + blk.num_pages * PAGE_SIZE > virt_addr
            && blk.flags & PageDescriptor::VIRTUAL != 0 {
                return Some(blk.start_phy_address + virt_addr - blk.start_virt_address);
            }
        }

        None
    }

    fn get_virt_address(&mut self, phys_addr: usize, fetch_type: MapFetchType) -> Option<usize> {
        // Check all locations linearly to get the virtual address
        let mut lowest_address = None;
        for blk in self.alloc_block_list.iter() {
            if blk.start_phy_address <= phys_addr && blk.start_phy_address + blk.num_pages * PAGE_SIZE > phys_addr 
            && blk.flags & PageDescriptor::VIRTUAL != 0 {
                let new_addr = blk.start_virt_address + phys_addr - blk.start_phy_address;  
                if lowest_address.is_none() {
                    lowest_address = Some(new_addr);
                }
                else {
                    lowest_address = lowest_address.and_then(|addr: usize| {
                        // In this case unconditionally set the preferred address as the kernel address even though previous address is lower
                        if fetch_type == MapFetchType::Kernel && new_addr >= KERNEL_HALF_OFFSET && addr < KERNEL_HALF_OFFSET {
                            Some(new_addr)
                        }
                        else {
                            Some(addr.min(new_addr))
                        }
                    });
                }
            }
        }

        #[cfg(debug_assertions)]
        if lowest_address.is_none() {
            debug!("phys_addr={}, alloc_block_list={:?}", phys_addr, self.alloc_block_list);
        }

        lowest_address
    }

    // Allowed to map memory only if the address is not already allocated
    // In case user wants to map new physical address to existing virtual address, then first unmap the memory
    // and then map the new physical address 
    fn map_memory(&mut self, phys_addr: usize, virt_addr: usize, size: usize, flags: u8) -> Result<(), KError> {
        if size == 0 {
            return Ok(())
        }

        let size = size + (size as *const u8).align_offset(PAGE_SIZE);
    
        if phys_addr & (PAGE_SIZE - 1) != 0 || virt_addr & (PAGE_SIZE - 1) != 0 
        || (flags & PageDescriptor::NO_ALLOC) != 0 {
            return Err(KError::InvalidArgument);
        }

        // Try to find a block which is either allocated but not mapped, or that is not allocated
        // The range we're interested in should be a superset of the region the user provided
        let blk = self.alloc_block_list.iter().chain(self.free_block_list.iter()).find(|item| {
            virt_addr >= item.start_virt_address 
            && virt_addr < item.start_virt_address + item.num_pages * PAGE_SIZE 
            && virt_addr + size <= item.start_virt_address + item.num_pages * PAGE_SIZE
            && (item.flags & PageDescriptor::NO_ALLOC != 0 || item.flags == 0)
        });
        
        // Ensure that resulting block is described as virtual memory
        let flags = flags | PageDescriptor::VIRTUAL; 
        if let Some(desc) = blk {
            let top = PageDescriptor {
                num_pages: ceil_div(virt_addr - desc.start_virt_address, PAGE_SIZE),
                start_phy_address: 0,
                start_virt_address: desc.start_virt_address,
                flags: desc.flags 
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
                flags: desc.flags
            };
            
            // Need to copy over the flags here to not cause borrowing issues
            let flags = desc.flags;
            if flags == 0 {
                unsafe {
                    self.free_block_list.remove_node(NonNull::from(desc));
                }
            }
            else {
                unsafe {
                    self.alloc_block_list.remove_node(NonNull::from(desc));
                }
            }

            for descriptor in [top, bottom] {
                if descriptor.num_pages != 0 {
                    if flags == 0 {
                        self.free_block_list.add_node(descriptor).expect(ERROR_MESSAGE);
                    }
                    else {
                        self.alloc_block_list.add_node(descriptor).expect(ERROR_MESSAGE);
                    }
                }
            }

            self.alloc_block_list.add_node(middle).expect(ERROR_MESSAGE);
        }
        else {
            debug!("alloc_block_list={:?}, free_block_list={:?}", self.alloc_block_list, self.free_block_list);
            info!("map_memory could not reserve memory of size:{} at address:{:#X}", size, virt_addr);
            return Err(KError::OutOfMemory);
        }
        
        self.page_mapper.map_memory(virt_addr, phys_addr, size, flags);
        Ok(())
    }

    fn unmap_memory(&mut self, virt_addr: *mut u8, size: usize) -> Result<(), KError> {
        let size = size + (size as *const u8).align_offset(PAGE_SIZE);  
        
        let num_pages = ceil_div(size, PAGE_SIZE);
        if virt_addr as usize & (PAGE_SIZE - 1) != 0 {
            return Err(KError::InvalidArgument);
        }

        let blk = self.alloc_block_list.iter().find(|item| {
            item.start_virt_address == virt_addr as usize && item.num_pages == num_pages && item.flags & PageDescriptor::VIRTUAL != 0 
        });
        
        
        if let Some(desc) = blk {
            unsafe {
                self.alloc_block_list.remove_node(NonNull::from(desc));
            }
        }
        else {
            // In case caller tries to free memory which has not been allocated, then we return here
            debug!("{:?}", self.alloc_block_list);
            return Err(KError::InvalidArgument);
        } 
        
        self.page_mapper.unmap_memory(virt_addr as usize, size);
        self.coalesce_block(virt_addr as usize, num_pages);
    
        Ok(())
    }
}

fn get_active_vcb() -> Option<NonNull<Spinlock<VirtMemConBlk>>> {
    let vcb = PER_CPU_ACTIVE_VCB.local().load(Ordering::Acquire);

    if vcb.is_null() {
        return None;
    }

    Some(NonNull::new(vcb).unwrap())
}

// Central API to call into both physical and virtual allocator
pub fn allocate_memory(layout: Layout, flags: u8) -> Result<*mut u8, KError> {
    let active_vcb = get_active_vcb();
    if (flags & PageDescriptor::VIRTUAL != 0) && active_vcb.is_some() {
        let allocator = active_vcb.unwrap();
        unsafe {
            (*allocator.as_ptr()).lock().allocate(layout, flags)
        }
    }
    else {
        // Perform only physical allocation
        PHY_MEM_CB.get().unwrap().lock().allocate(layout)
    }
}

pub fn deallocate_memory(addr: *mut u8, layout: Layout, flags: u8) -> Result<(), KError> {
    let active_vcb = get_active_vcb();
    if (flags & PageDescriptor::VIRTUAL != 0) && active_vcb.is_some() {
        let allocator = active_vcb.unwrap();
        unsafe {
            (*allocator.as_ptr()).lock().deallocate(addr, layout)
        }
    }
    else {
        PHY_MEM_CB.get().unwrap().lock().deallocate(addr, layout)
    }
}

pub fn get_physical_address(virt_addr: usize) -> Option<usize> {
    let active_vcb = get_active_vcb();
    if likely(active_vcb.is_some()) {
        unsafe {
            (*active_vcb.unwrap().as_ptr()).lock().get_phys_address(virt_addr)
        }
    }
    else {
        // Since virtual_mem = physical_mem
        Some(virt_addr)
    }
}

// Unlike get_physical_address, multiple virtual addresses could be mapped to the same physical address
// fetch type allows user to filter out the particular region they want
// Following rules are applicable only when there is more than one virtual address for given physical address
// Kernel -> If present, fetch the lowest address that is > KERNEL_HALF_OFFSET
// Any -> Fetch the lowest virtual address region
pub fn get_virtual_address(phys_addr: usize, fetch_type: MapFetchType) -> Option<usize> {
    let active_vcb = get_active_vcb();
    if likely(active_vcb.is_some()) {
        unsafe {
            (*active_vcb.unwrap().as_ptr()).lock().get_virt_address(phys_addr, fetch_type)
        }
    }
    else {
        // Since virtual_mem = physical_mem
        Some(hal::canonicalize_virtual(phys_addr))
    }
}

pub fn map_memory(phys_addr: usize, virt_addr: usize, size: usize, flags: u8) -> Result<(), KError> {
    let active_vcb = get_active_vcb();
    if likely(active_vcb.is_some()) {
        unsafe {
            (*active_vcb.unwrap().as_ptr()).lock().map_memory(phys_addr, virt_addr, size, flags)
        }
    }
    else {
        Ok(())
    }
}

// Here we also need to add type of memory we're unmapping (kernel or user)
// This is important, since if it's kernel memory, we need to unmap that memory in all address spaces
pub fn unmap_memory(virt_addr: *mut u8, size: usize) -> Result<(), KError> {
    let active_vcb = get_active_vcb();
    if likely(active_vcb.is_some()) {
        unsafe {
            (*active_vcb.unwrap().as_ptr()).lock().unmap_memory(virt_addr, size)
        }
    }
    else {
        Ok(())
    }
}
#[no_mangle]
extern "C" fn map_memory_ffi(phys_addr: usize, virt_addr: usize, size: usize, flags: u8) -> KError {
    map_memory(phys_addr, virt_addr, size, flags).into()
}

#[no_mangle]
extern "C" fn unmap_memory_ffi(virt_addr: *mut u8, size: usize) -> KError {
    unmap_memory(virt_addr, size).into() 
}

#[no_mangle]
extern "C" fn allocate_memory_ffi(size: usize, align: usize, flags: u8) -> KError {
    allocate_memory(Layout::from_size_align(size, align).unwrap(), flags).into()
}

#[no_mangle]
extern "C" fn deallocate_memory_ffi(addr: *mut u8, size: usize, align: usize, flags: u8) -> KError {
    deallocate_memory(addr, Layout::from_size_align(size, align).unwrap(), flags).into()
}

pub fn virtual_allocator_init() {
    // Create the kernel address space and attach it to first node in address space list
    let remap_list = REMAP_LIST.lock();

    let mut kernel_addr_space = VirtMemConBlk::new(true);
    // First map the identity mapped regions
    // In case, identity mapped region straddles the kernel upper half, the checks within function will halt kernel
    // We can take it up later
    remap_list.iter().filter(|item| {
        matches!(item.map_type, IdentityMapped)
    }).for_each(|item| {
        info!("Identity mapping region of size:{} with physical address:{:#X}", 
        item.value.size, item.value.base_address);
        kernel_addr_space.map_memory(
            item.value.base_address, item.value.base_address, 
            item.value.size, item.flags).unwrap();
    });

    // Now map remaining set of regions onto upper half of memory
    remap_list.iter().filter(|item| {
        !matches!(item.map_type, IdentityMapped)
    }).for_each(|item| {
        let layout = Layout::from_size_align(item.value.size, PAGE_SIZE).unwrap();
        let virt_addr = kernel_addr_space.allocate(layout, PageDescriptor::VIRTUAL | PageDescriptor::NO_ALLOC)
        .expect("System could not find suitable memory in higher half kernel space") as usize;
        
        info!("Mapping region of size:{} with physical address:{:#X} to virtual address:{:#X}", 
        item.value.size, item.value.base_address, virt_addr);

        kernel_addr_space.map_memory(item.value.base_address, virt_addr, item.value.size, item.flags).unwrap();
        
        // Update user of new location
        if let OffsetMapped(f) = &item.map_type {
            f(virt_addr);
        }
    });
    
    // Create a new stack for boot cpu
    let stack_raw = kernel_addr_space.allocate(Layout::from_size_align(cpu::TOTAL_STACK_SIZE, PAGE_SIZE).unwrap()
    , PageDescriptor::VIRTUAL | PageDescriptor::NO_ALLOC).expect("Failed to create space in virtual address for boot cpu stack");

    let stack_raw_phys = PHY_MEM_CB.get().unwrap().lock().allocate(Layout::from_size_align(cpu::INIT_STACK_SIZE, PAGE_SIZE).unwrap())
    .expect("Failed to create space for physical address space for boot cpu stack");

    #[cfg(feature = "stack_down")]
    let stack_base = unsafe {
        stack_raw.add(cpu::INIT_GUARD_PAGE_SIZE)
    };

    #[cfg(not(feature = "stack_down"))]
    let stack_base = stack_raw;

    kernel_addr_space.map_memory(stack_raw_phys.addr(), stack_base.addr(), cpu::INIT_STACK_SIZE, PageDescriptor::VIRTUAL)
    .expect("Failed to map boot cpu stack to kernel virtual address space!");

    debug!("Created boot cpu stack with virtual address: {:#X} and physical address: {:#X}", stack_base.addr(), stack_raw_phys.addr());

    cpu::set_worker_stack_for_boot_cpu(stack_raw);

    // Finalize address space creation
    kernel_addr_space.page_mapper.bootstrap_activate();

    ADDRESS_SPACES.call_once(|| {
        let mut l = List::new();
        l.add_node(Spinlock::new(kernel_addr_space)).unwrap();

        Spinlock::new(l)
    }); 

    // The pointer referenced here will never be moved (Atleast, not the kernel base address space (The first address space))
    PER_CPU_ACTIVE_VCB.local().store(
        ptr_to_ref_mut(&**ADDRESS_SPACES.get().unwrap().lock().first().unwrap()),
        Ordering::Release
    );   

    info!("Created kernel address space"); 
}

#[cfg(test)]
pub fn virtual_allocator_test() {
    let mut allocator = VirtMemConBlk::new(false);

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

pub fn on_page_fault(fault_address: usize) {
    panic!("Page fault exception!\nFault address:{:#X}", fault_address);
}