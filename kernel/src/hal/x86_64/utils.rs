use super::asm;
use crate::logger::debug;

#[derive(Debug, Clone, Copy)]
pub struct VirtAddr(u64);

impl VirtAddr {
    pub fn new(addr: usize) -> Self {
        Self (Self::canonicalize(addr as u64))
    }

    // Virtual address in AMD/Intel for 64 bit mode is 48 bits. All upper bits must match 47th bit
    fn canonicalize(mut addr: u64) -> u64 {
        if addr & (1 << 47) != 0 {
            addr |= (0xffff as u64) << 48;
        }
        else {
            addr &= !((0xffff as u64) << 48);
        }

        addr
    }

    fn get(&self) -> usize {
        self.0 as usize
    }
}



pub fn switch_stack_and_jump(stack_address: VirtAddr, kernel_address: VirtAddr) -> ! {
    
    debug!("Init Stack address:{:#X}", stack_address.get());
    debug!("Kern_main:{:#X}", kernel_address.get());

    unsafe {
        asm::switch_stack_and_jump(stack_address.get() as u64,  kernel_address.get() as u64);
        asm::halt();
    }
}