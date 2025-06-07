#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests;
mod arch;

use common::{elf::*, *};
use arch::*;

pub fn load_kernel(kernel_file: *const u8) -> KernelInfo {
    let signature = unsafe {
        *(kernel_file as *const u32)
    };

    if signature != ELFMAG {
        panic!("Invalid signature for kernel elf file = {}!", signature);
    }

    let elf_hdr = unsafe {
        &*(kernel_file as *const Elf64Ehdr)
    };


    test_log!("{:?}", elf_hdr);

    load_kernel_arch(kernel_file, elf_hdr)
}

