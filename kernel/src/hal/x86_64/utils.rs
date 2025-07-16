use super::asm;
use crate::debug;

#[derive(Debug, Clone, Copy)]
pub struct VirtAddr(u64);

impl VirtAddr {
    #[cfg(not(test))]
    #[inline(always)]
    pub fn new(addr: usize) -> Self {
        // Virtual address in AMD/Intel for 64 bit mode is 48 bits. All upper bits must match 47th bit
        Self (Self::canonicalize(addr as u64, 47))
    }
    
    #[cfg(test)]
    pub fn new(addr: usize) -> Self {
        Self(addr as u64)
    }

    #[inline(always)]
    fn canonicalize(mut addr: u64, last_bit: u8) -> u64 {
        if addr & (1 << last_bit) != 0 {
            addr |= (0xffff as u64) << (last_bit + 1);
        }
        else {
            addr &= !((0xffff as u64) << (last_bit + 1));
        }

        addr
    }

    #[inline(always)]
    pub fn get(&self) -> usize {
        self.0 as usize
    }
}

pub fn canonicalize_virtual(addr: usize) -> usize {
    VirtAddr::new(addr).get()
}


pub fn switch_to_new_address_space(pml4_phys: usize, stack_address: usize, kernel_address: usize) -> ! {
    debug!("Init stack address = {:#X}", stack_address);
    debug!("kern_address_space_start address = {:#X}", kernel_address);

    // Special hook to tell logger to update it's internal pointers to new framebuffer address now
    crate::logger::relocate();
    
    unsafe {
        asm::init_address_space(pml4_phys as u64, stack_address as u64,  kernel_address as u64);
        
        // Shouldn't reach here
        asm::halt();
    }
}