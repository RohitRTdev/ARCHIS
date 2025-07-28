use super::asm;
use kernel_intf::debug;

#[inline(always)]
#[no_mangle]
pub extern "C" fn read_timestamp() -> usize {
    unsafe {
        asm::rdtsc() as usize
    }
}


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
    debug!("kern_address_space_start address = {:#X}", kernel_address);

    // Special hooks to tell logger and cpu to update it's internal pointers now
    crate::cpu::relocate_cpu_init_stack(); 
    crate::logger::relocate_framebuffer();
    unsafe {
        asm::init_address_space(pml4_phys as u64, stack_address as u64,  kernel_address as u64);
        
        // Shouldn't reach here
        asm::halt();
    }
}