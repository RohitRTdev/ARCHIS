use core::alloc::Layout;
use core::sync::atomic::{Ordering, AtomicUsize};
use core::hint::unlikely;
use crate::mem::MapFetchType;
use crate::cpu;
use crate::{hal::x86_64::features::CPU_FEATURES, mem};
use crate::hal::{VirtAddr, notify_core};
use kernel_intf::info;
use common::{PAGE_SIZE, ceil_div, en_flag, usize_to_ref_mut};
use super::asm;
use super::IPIRequestType;

struct PTE;

impl PTE {
    pub const P: u64 = 1;
    pub const RW: u64 = 1 << 1;
    pub const U: u64 = 1 << 2;
    pub const PWT: u64 = 1 << 3;
    pub const PCD: u64 = 1 << 4;
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

static KERNEL_PML4: AtomicUsize = AtomicUsize::new(0);
static mut DISABLE_INVALIDATION: bool = true;

pub struct PageMapper {
    pml4_phys: u64, 
    is_current: bool,
    proc_id: usize,
    is_allocated: bool
}

const RECURSIVE_SLOT: u64 = 511;
const TOTAL_ENTRIES: usize = 512;

impl PageMapper {
    #[cfg(not(test))]
    pub fn new(is_kernel_pml4: bool, proc_id: usize) -> Self {
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        let pml4_phys = mem::allocate_memory(layout, 0)
                                .expect("Page base table allocation failed!");
        
        let pml4 = mem::map_page_table(pml4_phys.addr(), proc_id).expect("Failed to map pml4 to process address space");
        info!("Creating new address space with pml4 virtual address:{:#X} and physical address:{:#X}", pml4, pml4_phys.addr());

        // Initialize the page table (Recursive mapping)
        unsafe {
            let raw_addr = pml4 as *mut u64;
            raw_addr.write_bytes(0, TOTAL_ENTRIES);
            *raw_addr.add(RECURSIVE_SLOT as usize) = (pml4_phys as u64 & PTE::PHY_ADDR_MASK) | PTE::PWT | PTE::RW | PTE::P; 
        }

        if is_kernel_pml4 {
            KERNEL_PML4.store(pml4_phys.addr(), Ordering::SeqCst);
        }

        Self {
            pml4_phys: pml4_phys as u64,
            is_current: false,
            proc_id,
            is_allocated: false
        }
    }

    #[cfg(test)]
    pub fn new(_: bool, _: usize) -> Self {
        Self {
            pml4_phys: 0,
            is_current: false,
            proc_id: 0,
            is_allocated: false
        }
    }

    pub fn set_allocated(&mut self) {
        self.is_allocated = true;
    }

    fn set_current(&mut self) {
        let pml4 = unsafe {
            asm::read_cr3() & PTE::PHY_ADDR_MASK
        };

        self.is_current = pml4 == self.pml4_phys;
    }

    pub fn set_address_space(&mut self) {
        self.is_current = true;
        core::sync::atomic::compiler_fence(Ordering::SeqCst);
        info!("Setting address space for proc {}", self.proc_id);
        unsafe {
            // Set page table as write through
            asm::write_cr3((self.pml4_phys & PTE::PHY_ADDR_MASK) | PTE::PWT);
        }
    }

    pub fn map_memory(&mut self, virt_addr: usize, phys_addr: usize, size: usize, flags: u8) {
        assert!(virt_addr & 0xfff == 0  && phys_addr & 0xfff == 0 && size & 0xfff == 0);
        let num_pages = ceil_div(size, PAGE_SIZE);
        self.set_current();
        for i in 0..num_pages {
            let va = virt_addr + i * PAGE_SIZE;
            let pa = phys_addr + i * PAGE_SIZE;
            self.map_page(va as u64, pa as u64, flags & mem::PageDescriptor::USER != 0, 
            flags & mem::PageDescriptor::MMIO != 0);
        }
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    pub fn unmap_memory(&mut self, virt_addr: usize, size: usize) {
        assert!(virt_addr & 0xfff == 0 && size & 0xfff == 0);
        let num_pages = ceil_div(size, PAGE_SIZE);
        self.set_current();
        for i in 0..num_pages {
            let va = virt_addr + i * PAGE_SIZE;
            self.unmap_page(va as u64);
        }
        core::sync::atomic::fence(Ordering::SeqCst);
    }

    // This is not a perfect solution. There is a brief period (The time from which these IPIs are sent
    // and the time they are received by the other cores), where the other cores won't observe the new mapping.
    // However this only becomes a problem if 2 or more cores are 
    // a) sharing the same address space
    // b) actively working with a region of virtual memory whose mapping has been currently changed on one of the cores
    // Unless we're changing the physical mapping of one of the core regions that are mapped during page map init,
    // we won't run into situation b
    // In other cases, two tasks won't be working with same physical memory
    // The one legitimate case where this is true is for shared memory (shmem). However, we don't have this concept yet in the kernel.
    // The different cores will require some external synchronization mechanism to make it work. For now, this will do.
    pub fn invalidate_other_cores() {
        core::sync::atomic::fence(Ordering::SeqCst);
        let cur_core = super::get_core();
        let total_cores = cpu::get_total_cores();
        
        if unlikely(unsafe{DISABLE_INVALIDATION}) {
            return;
        }

        for core in 0..total_cores {
            if core != cur_core {
                notify_core(IPIRequestType::TlbInvalidate, core);
            }
        }
    }

    fn invalidate_tlb(&self, virt_addr: usize) {
        if self.is_current {
            unsafe { asm::invlpg(VirtAddr::new(virt_addr).get() as u64); }
        }
    }

    fn unmap_page(&mut self, virt_addr: u64) {
        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::split_indices(virt_addr);
        let mut pml_addr = 0;
        let mut pdpt_addr = 0;
        let mut pd_addr = 0;
        let pt = if !self.is_current {
            unsafe {
                pml_addr = mem::map_page_table(self.pml4_phys as usize, self.proc_id).expect("Page table could not be mapped to process address space");
                let pdpt = *(pml_addr as *mut u64).add(pml4_idx) & PTE::PHY_ADDR_MASK;
                pdpt_addr = mem::map_page_table(pdpt as usize, self.proc_id).expect("Page table could not be mapped to process address space");
                let pd = *(pdpt_addr as *mut u64).add(pdpt_idx) & PTE::PHY_ADDR_MASK;
                pd_addr = mem::map_page_table(pd as usize, self.proc_id).expect("Page table could not be mapped to process address space");
                let pt = *(pd_addr as *mut u64).add(pd_idx) & PTE::PHY_ADDR_MASK;
                let pt_addr = mem::map_page_table(pt as usize, self.proc_id).expect("Page table could not be mapped to process address space");
                
                usize_to_ref_mut::<[u64; TOTAL_ENTRIES]>(pt_addr)
            }
        }
        else {
            self.get_table_mut(PageLevel::PT, pml4_idx, pdpt_idx, pd_idx, 0)
        };

        // Unmap this entry
        assert!(pt[pt_idx] & PTE::P != 0);
        pt[pt_idx] = 0;

        self.invalidate_tlb(virt_addr as usize);
        self.unmap_page_tables(pml_addr, pdpt_addr, pd_addr, pt.as_ptr().addr());

        // TODO:
        // Unmap the page tables also incase they become empty?
    }

    fn map_page(&mut self, virt_addr: u64, phys_addr: u64, is_user: bool, is_mmio: bool) {
        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::split_indices(virt_addr);

        let pml4 = self.get_table_mut(PageLevel::PML4, 0, 0, 0, self.pml4_phys as usize);
        let pdpt = self.get_or_alloc_table(pml4, pml4_idx, PageLevel::PDPT, pml4_idx, 0, 0);
        let pd = self.get_or_alloc_table(pdpt, pdpt_idx, PageLevel::PD, pml4_idx, pdpt_idx, 0);
        let pt = self.get_or_alloc_table(pd, pd_idx, PageLevel::PT, pml4_idx, pdpt_idx, pd_idx);

        pt[pt_idx] = (phys_addr & PTE::PHY_ADDR_MASK) | en_flag!(is_user, PTE::U) | 
        en_flag!(is_mmio, PTE::PCD) | en_flag!(is_mmio, PTE::PWT) | en_flag!(!is_user && CPU_FEATURES.get().unwrap().lock().pge, PTE::G) 
        | PTE::RW | PTE::P; 
        
        self.invalidate_tlb(virt_addr as usize);
        self.unmap_page_tables(pml4.as_ptr().addr(), pdpt.as_ptr().addr(), pd.as_ptr().addr(), pt.as_ptr().addr());
    }

    fn unmap_page_tables(&mut self, pml4: usize, pdpt: usize, pd: usize, pt: usize) {
        if self.is_current {
            return;
        }

        mem::unmap_page_table(pml4, self.proc_id).expect("Failed to unmap pml4 from process address space");        
        mem::unmap_page_table(pdpt, self.proc_id).expect("Failed to unmap pdpt from process address space");        
        mem::unmap_page_table(pd, self.proc_id).expect("Failed to unmap pd from process address space");        
        mem::unmap_page_table(pt, self.proc_id).expect("Failed to unmap pt from process address space");        
    }

    // Get a mutable reference to a page table at a given level and index using recursive mapping
    // If this address space is not active, then caller is expected to fetch the virtual address to which this page table is mapped
    // level -> Indicates which level page table user wants to access
    fn get_table_mut(&self, level: PageLevel, pml_idx: usize, pdpt_idx: usize, pd_idx: usize, phy_addr: usize) -> &mut [u64; 512] {
        let virt = if self.is_current {
            match level {
                PageLevel::PML4 => Self::recursive_map_addr(RECURSIVE_SLOT, RECURSIVE_SLOT, RECURSIVE_SLOT),
                PageLevel::PDPT => Self::recursive_map_addr(RECURSIVE_SLOT, RECURSIVE_SLOT, pml_idx as u64),
                PageLevel::PD => Self::recursive_map_addr(RECURSIVE_SLOT, pml_idx as u64, pdpt_idx as u64),
                PageLevel::PT => Self::recursive_map_addr(pml_idx as u64, pdpt_idx as u64, pd_idx as u64)
            }
        }
        else {
            mem::map_page_table(phy_addr, self.proc_id).expect("Page table allocation failed!") as u64
        };
        
        unsafe { &mut *(virt as *mut [u64; 512]) }
    }

    // Get or allocate the next-level table, and ensure it is mapped in the recursive region
    // table -> Parent table from which we're going to obtain the next level page table
    // idx -> index in parent table to which the next level page table points
    // level -> Should be the next level page table we want
    // Ex: if table is PDPT, then level must be PD and idx must be the PDPT entry that points to that PD
    fn get_or_alloc_table(&self, table: &mut [u64; 512], idx: usize, level: PageLevel, pml_idx: usize, pdpt_idx: usize, pd_idx: usize) -> &mut [u64; 512] {
        // Get the virtual address of the table we're interested in
        // If page table is not present, then allocate it first
        let addr = if table[idx] & 1 == 0 {
            let addr = self.allocate_page_table();
            
            // Map the physical address to the upper level table
            table[idx] = addr.1 as u64 & PTE::PHY_ADDR_MASK
            | PTE::U | PTE::PWT | PTE::P | PTE::RW;
            Some(addr)
        }
        else {
            None
        };
        let vaddr = if self.is_current {
            // This address is valid if this address space were active
            let rec_addr = match level {
                PageLevel::PDPT => Self::recursive_map_addr(RECURSIVE_SLOT, RECURSIVE_SLOT, pml_idx as u64),
                PageLevel::PD => Self::recursive_map_addr(RECURSIVE_SLOT, pml_idx as u64, pdpt_idx as u64),
                PageLevel::PT => Self::recursive_map_addr(pml_idx as u64, pdpt_idx as u64, pd_idx as u64),
                _ => {
                    panic!("get_or_alloc_table() called with level: PML4");
                }
            } as usize;
            
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
                // Page table was already exists in physical memory.
                // Map it to current process's address space
                let phys = table[idx] & PTE::PHY_ADDR_MASK;
                mem::map_page_table(phys as usize, self.proc_id).expect("Page table could not be mapped to current process space")
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
        unsafe { &mut *(vaddr as *mut [u64; 512]) }
    }

    pub fn get_table_mapping(&mut self, virt_addr: u64) -> usize {
        self.set_current();

        assert!(self.is_current);

        let (pml4_idx, pdpt_idx, pd_idx, pt_idx) = Self::split_indices(virt_addr);
        let pt = self.get_table_mut(PageLevel::PT, pml4_idx, pdpt_idx, pd_idx, 0);

        let phy_addr = pt[pt_idx] as usize;

        phy_addr
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
            let phy_addr = mem::allocate_memory(layout, 0).expect("Page table allocation failed!") as usize;
            
            // Otherwise, allocate it somewhere in caller's virtual memory and fetch the page
            let virt_addr = mem::map_page_table(phy_addr, self.proc_id).expect("Page table could not be mapped to current process space");
            (virt_addr, phy_addr)
        }
    }

    // Compute the recursive mapping address for a page table at a given level and indices
    fn recursive_map_addr(pml: u64, pdpt: u64, pd: u64) -> u64 {
        // Since memory address needs to be canonical, we use 0xffffff instead of 0x1ff
        (0x1ffffff << 39) |
        ((pml & 0x1ff) << 30) |
        ((pdpt & 0x1ff) << 21) |
        ((pd & 0x1ff) << 12)
    }
    
    fn split_indices(virt_addr: u64) -> (usize, usize, usize, usize) {
        let pml4 = (virt_addr >> 39) & 0x1ff;
        let pdpt = (virt_addr >> 30) & 0x1ff;
        let pd = (virt_addr >> 21) & 0x1ff;
        let pt = (virt_addr >> 12) & 0x1ff;
        (pml4 as usize, pdpt as usize, pd as usize, pt as usize)
    }

    // We must have been switched out from this address space
    pub fn destroy_page_tables(&mut self) {
        self.set_current();

        assert!(self.proc_id != 0, "Attempted to destroy kernel address space!");
        assert!(!self.is_current, "Address space being destroyed while currently selected!");
        assert!(self.is_allocated, "destroy_page_table called on destroyed address space!");

        let pml4 = usize_to_ref_mut::<[u64; TOTAL_ENTRIES]>(mem::map_page_table(self.pml4_phys as usize, self.proc_id)
        .expect("Unable to map pml4 to process address space"));
        
        let mut page_tables = 0;
        info!("Destroying page tables for process {}", self.proc_id);
        
        // Go over PML4 entries 
        let layout = Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap();
        // Skip the last entry. That is the recursive mapping
        // It contains the PML4 address itself, which we deallocate at the end
        for pml4_entry in 0..TOTAL_ENTRIES-1 {
            if (pml4[pml4_entry] & PTE::P) == 0 {
                continue;
            }

            let pdpt_phy = pml4[pml4_entry] & PTE::PHY_ADDR_MASK;
            let pdpt = usize_to_ref_mut::<[u64; TOTAL_ENTRIES]>(mem::map_page_table(pdpt_phy as usize, self.proc_id)
            .expect("Unable to map pdpt to process address space"));

            for pdpt_entry in 0..TOTAL_ENTRIES {
                if (pdpt[pdpt_entry] & PTE::P) == 0 {
                    continue;
                }

                let pd_phy = pdpt[pdpt_entry] & PTE::PHY_ADDR_MASK;
                let pd = usize_to_ref_mut::<[u64; TOTAL_ENTRIES]>(mem::map_page_table(pd_phy as usize, self.proc_id)
                .expect("Unable to map pd to process address space"));

                for pd_entry in 0..TOTAL_ENTRIES {
                    if (pd[pd_entry] & PTE::P) == 0 {
                        continue;
                    }

                    let pt_phy = pd[pd_entry] & PTE::PHY_ADDR_MASK;
                    // Deallocate PT
                    mem::deallocate_memory(pt_phy as *mut u8, layout, 0).expect("Failed to deallocate PT"); 
                    page_tables += 1;
                }

                // All entries within this PD have been deallocated. Now, deallocate this PD
                mem::unmap_page_table(pd.as_ptr().addr(), self.proc_id).expect("Failed to unmap PD from process address space");
                
                // Deallocate PD
                mem::deallocate_memory(pd_phy as *mut u8, layout, 0).expect("Failed to deallocate PD"); 
                page_tables += 1;
            }
            
            // All entries within this PDPT have been deallocated. Now, deallocate this PDPT
            mem::unmap_page_table(pdpt.as_ptr().addr(), self.proc_id).expect("Failed to unmap PDPT from process address space");
            
            // Deallocate PDPT
            mem::deallocate_memory(pdpt_phy as *mut u8, layout, 0).expect("Failed to deallocate PDPT"); 
            page_tables += 1;
        }
        
        // All entries within PML4 have been deallocated.
        mem::unmap_page_table(pml4.as_ptr().addr(), self.proc_id).expect("Failed to unmap PML4 from process address space");
        
        // Deallocate PML4
        mem::deallocate_memory(self.pml4_phys as *mut u8, layout, 0).expect("Failed to deallocate PML4"); 
        page_tables += 1;

        self.is_allocated = false;
    
        info!("Destroyed {} page tables", page_tables);
    }
}

pub fn enable_invalidation() {
    unsafe {
        DISABLE_INVALIDATION = false;
    }
}

pub fn get_kernel_pml4() -> usize {
    KERNEL_PML4.load(Ordering::SeqCst)
}