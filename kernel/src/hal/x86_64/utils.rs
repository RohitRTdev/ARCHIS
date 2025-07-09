use super::asm;
use crate::hal::x86_64::features::CPU_FEATURES;
use crate::logger::debug;

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

#[derive(Debug, Clone, Copy)]
pub struct VirtAddr(u64);

#[derive(Debug, Clone, Copy)]
pub struct PhysAddr(u64);

impl VirtAddr {
    pub fn new(addr: usize) -> Self {
        // Virtual address in AMD/Intel for 64 bit mode is 48 bits. All upper bits must match 47th bit
        Self (canonicalize(addr as u64, 47))
    }

    pub fn get(&self) -> usize {
        self.0 as usize
    }
}

impl PhysAddr {
    pub fn new(addr: usize) -> Self {
        Self (canonicalize(addr as u64, CPU_FEATURES.get().unwrap().lock().phy_addr_width - 1))
    }

    pub fn get(&self) -> usize {
        self.0 as usize
    }
}

pub fn canonicalize_physical(addr: usize) -> usize {
    PhysAddr::new(addr).get()
}

pub fn canonicalize_virtual(addr: usize) -> usize {
    VirtAddr::new(addr).get()
}


pub fn switch_stack_and_jump(stack_address: VirtAddr, kernel_address: VirtAddr) -> ! {
    
    debug!("Init stack address:{:#X}", stack_address.get());
    debug!("kern_main:{:#X}", kernel_address.get());

    unsafe {
        asm::switch_stack_and_jump(stack_address.get() as u64,  kernel_address.get() as u64);
        
        // Shouldn't reach here
        asm::halt();
    }
}