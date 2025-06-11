#![cfg_attr(not(test), no_std)]

#[cfg(test)]
mod tests;
mod arch;

use common::{elf::*, *};
use arch::*;

pub const KERNEL_FILE: &str = "sys/aris.elf";

pub const ROOT_FILES: [&str; 3] = [
    KERNEL_FILE,
    "sys/drivers/libtest1.so",
    "sys/drivers/libtest2.so"
];

pub fn jump_to_kernel(boot_info: &BootInfo) -> ! {
    let kern_entry = &boot_info.kernel_desc.entry as *const _  as *const extern "sysv64" fn(*const BootInfo) -> !;

    unsafe {
        (*kern_entry)(boot_info as *const BootInfo)
    }
}


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

