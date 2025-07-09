use core::alloc::Layout;
use crate::{hal::x86_64::features::CPU_FEATURES, mem};
use crate::hal::{VirtAddr, PhysAddr};
use crate::logger::info;
use common::{align_down, ceil_div, en_flag, PAGE_SIZE};
use super::asm;
struct PTE;

impl PTE {
    pub const P: u64 = 1;
    pub const RW: u64 = 1 << 1;
    pub const U: u64 = 1 << 2;
    pub const PWT: u64 = 1 << 3;
    pub const G: u64 = 1 << 8;
    pub const PHY_ADDR_MASK: u64 = 0x000fffff_fffff000;
}

#[derive(Debug, Clone, Copy)]
enum PageLevel {
    PML4,
    PDPT,
    PD,
    PT
}

pub struct PageMapper {
    pml4_phys: usize, 
    is_current: bool, 
}

const RECURSIVE_SLOT: u64 = 511;
const TOTAL_ENTRIES: usize = 512;

impl PageMapper {
    pub fn new() -> Self {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        let pml4 = mem::allocate_memory(layout, mem::PageDescriptor::VIRTUAL)
                                .expect("Page base table allocation failed!") as usize;
        
        let pml4_phy = mem::get_physical_address(pml4).unwrap();
        info!("Creating new address space with pml4 virtual address:{:#X} and physical address:{:#X}", pml4, pml4_phy);

        // Initialize the page table (Recursive mapping)
        unsafe {
            let raw_addr = pml4 as *mut u64;
            raw_addr.write_bytes(0, TOTAL_ENTRIES);
            *raw_addr.add(RECURSIVE_SLOT as usize) = PhysAddr::new(
                ((pml4_phy as u64 & PTE::PHY_ADDR_MASK) | PTE::U | PTE::PWT | PTE::RW | PTE::P) as usize).get() as u64; 
        }

        Self {
            pml4_phys: pml4_phy,
            is_current: false
        }
    }
    
    
    pub fn map_memory(&mut self, virt_addr: usize, phys_addr: usize, size: usize, flags: u8) {
        let num_pages = ceil_div(size, PAGE_SIZE);
        for i in 0..num_pages {
            let va = align_down(virt_addr, PAGE_SIZE) + i * PAGE_SIZE;
            let pa = align_down(phys_addr, PAGE_SIZE) + i * PAGE_SIZE;
            self.map_page(va as u64, pa as u64, flags & mem::PageDescriptor::USER != 0);
        }
    }

    pub fn unmap_memory(&mut self, virt_addr: usize, size: usize) {
        let num_pages = ceil_div(size, PAGE_SIZE);
        for i in 0..num_pages {
            let va = align_down(virt_addr, PAGE_SIZE) + i * PAGE_SIZE;
            self.unmap_page(va as u64);
        }
    }

    fn unmap_page(&mut self, virt_addr: u64) {
        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::split_indices(virt_addr);
        let vaddr = if !self.is_current {
            unsafe {
                let pml_addr = mem::get_virtual_address(self.pml4_phys).expect("Page base table is expected to be mapped to caller virtual memory");
                let pdpt = *(pml_addr as *mut u64).add(pml4_idx) & PTE::PHY_ADDR_MASK;
                let pdpt_addr = mem::get_virtual_address(pdpt as usize).expect("PDPT is expected to be mapped to caller virtual memory");
                let pd = *(pdpt_addr as *mut u64).add(pdpt_idx) & PTE::PHY_ADDR_MASK;
                let pd_addr = mem::get_virtual_address(pd as usize).expect("PD is expected to be mapped to caller virtual memory");
                let pt = *(pd_addr as *mut u64).add(pd_idx) & PTE::PHY_ADDR_MASK;
                let pt_addr = mem::get_virtual_address(pt as usize).expect("PT is expected to be mapped to caller virtual memory");
                pt_addr
            }
        }
        else {
            0
        };

        let pt = self.get_table_mut(PageLevel::PT, pdpt_idx, pd_idx, pt_idx, vaddr);

        // Unmap this entry
        pt[pt_idx as usize] = 0;

        if self.is_current {
            unsafe { asm::invlpg(VirtAddr::new(virt_addr as usize).get() as u64); }
        }

        // TODO:
        // Unmap the page tables also incase they become empty?
    }

    fn map_page(&mut self, virt_addr: u64, phys_addr: u64, is_user: bool) {
        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::split_indices(virt_addr);

        let pml_base = if !self.is_current {
            mem::get_virtual_address(self.pml4_phys).expect("Page base table is expected to be mapped to caller virtual memory")
        }
        else {
            0
        };

        let pml4 = self.get_table_mut(PageLevel::PML4, 0, 0, 0, pml_base);
        let pdpt = self.get_or_alloc_table(pml4, pml4_idx, PageLevel::PDPT, pdpt_idx, 0, 0);
        let pd = self.get_or_alloc_table(pdpt, pdpt_idx, PageLevel::PD, pdpt_idx, pd_idx, 0);
        let pt = self.get_or_alloc_table(pd, pd_idx, PageLevel::PT, pdpt_idx, pd_idx, pt_idx);
        
        pt[pt_idx] = (PhysAddr::new(phys_addr as usize).get() as u64 & PTE::PHY_ADDR_MASK) | en_flag!(is_user, PTE::U) | 
        en_flag!(!is_user && CPU_FEATURES.get().unwrap().lock().pge, PTE::G) 
        | PTE::RW | PTE::P; 
        
        if self.is_current {
            unsafe { asm::invlpg(VirtAddr::new(virt_addr as usize).get() as u64); }
        }
    }

    // Get a mutable reference to a page table at a given level and index using recursive mapping
    // If this address space is not active, then caller is expected to fetch the virtual address to which this page table is mapped
    fn get_table_mut(&self, level: PageLevel, pdpt_idx: usize, pd_idx: usize, pt_idx: usize, vaddr: usize) -> &mut [u64; 512] {
        let virt = if self.is_current {
            match level {
                PageLevel::PML4 => Self::recursive_map_addr(RECURSIVE_SLOT, RECURSIVE_SLOT, RECURSIVE_SLOT),
                PageLevel::PDPT => Self::recursive_map_addr(RECURSIVE_SLOT, RECURSIVE_SLOT, pdpt_idx as u64),
                PageLevel::PD => Self::recursive_map_addr(RECURSIVE_SLOT, pdpt_idx as u64, pd_idx as u64),
                PageLevel::PT => Self::recursive_map_addr(pdpt_idx as u64, pd_idx as u64, pt_idx as u64)
            }
        }
        else {
            vaddr as u64
        };
        
        unsafe { &mut *(virt as *mut [u64; 512]) }
    }

    // Get or allocate the next-level table, and ensure it is mapped in the recursive region
    fn get_or_alloc_table(&self, table: &mut [u64; 512], idx: usize, level: PageLevel, pdpt_idx: usize, pd_idx: usize, pt_idx: usize) -> &mut [u64; 512] {
        // Get the virtual address of the table we're interested in
        // If page table is not present, then allocate it first
        let addr = if table[idx] & 1 == 0 {
            let addr = self.allocate_page_table();
            
            // Map the physical address to the upper level table
            table[idx] = (PhysAddr::new(addr.1).get() as u64 & PTE::PHY_ADDR_MASK)
            | PTE::U | PTE::PWT | PTE::P | PTE::RW;
            Some(addr)
        }
        else {
            None
        };
        let vaddr = if self.is_current {
            // This address is valid if this address space were active
            let rec_addr = Self::recursive_map_addr(pdpt_idx as u64, pd_idx as u64, pt_idx as u64) as usize;
            
            // If we had just mapped that memory, need to invalidate this region to make it visible
            if addr.is_some() {
                unsafe {
                    asm::invlpg(VirtAddr::new(rec_addr).get() as u64);
                }
            }

            rec_addr
        }
        else {
            if let Some(val) = &addr {
                val.0
            }
            else {
                // Page table was already existing. In this case, simply find what it is
                let phys = table[idx] & PTE::PHY_ADDR_MASK;
                mem::get_virtual_address(phys as usize).expect("Expected page table to have been mapped")
            }
        };

        // If table was allocated now, then initialize it
        if addr.is_some() {
            unsafe {
                (vaddr as *mut u64).write_bytes(0, TOTAL_ENTRIES);
            }
        }
        
        // Using the table's virtual address, get reference to actual table
        // This virtual address may be in recursive region or in some region in caller's memory
        self.get_table_mut(level, pdpt_idx, pd_idx, pt_idx, vaddr)
    }


    // Allocates 1 page table and returns it's virtual and physical memory
    fn allocate_page_table(&self) -> (usize, usize) {
        // If current active space, then just give the physical memory as caller will recursively map it
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        if self.is_current {
            let phy_addr = mem::allocate_memory(layout, 0).expect("Page table allocation failed!") as usize;
            (phy_addr, phy_addr)
        }
        else {
            // Otherwise, allocate it somewhere in caller's virtual memory and fetch the page
            let virt_addr = mem::allocate_memory(layout, mem::PageDescriptor::VIRTUAL).expect("Page table allocation failed!") as usize;
            
            let phy_addr = mem::get_physical_address(virt_addr).expect("Expected page table to have been mapped");
            (virt_addr, phy_addr)
        }
    }
    
    /// Compute the recursive mapping address for a page table at a given level and indices
    fn recursive_map_addr(pdpt: u64, pd: u64, pt: u64) -> u64 {
        // Since memory address needs to be canonical, we use 0xffffff instead of 0x1ff
        (0xffffff << 39) |
        ((pdpt & RECURSIVE_SLOT) << 30) |
        ((pd & RECURSIVE_SLOT) << 21) |
        ((pt & RECURSIVE_SLOT) << 12)
    }
    
    fn split_indices(virt_addr: u64) -> (usize, usize, usize, usize) {
        let pml4 = (virt_addr >> 39) & 0x1ff;
        let pdpt = (virt_addr >> 30) & 0x1ff;
        let pd = (virt_addr >> 21) & 0x1ff;
        let pt = (virt_addr >> 12) & 0x1ff;
        (pml4 as usize, pdpt as usize, pd as usize, pt as usize)
    }
}


