use common::{elf::*, ArrayTable};
use common::{MemoryRegion, ModuleInfo};
use crate::{RemapEntry, RemapType::*, BOOT_INFO, REMAP_LIST};
use crate::ds::{List, ListNode};
use crate::sync::Spinlock;
use crate::{info, debug};
use crate::mem::{self, FixedAllocator, Regions::*};

pub struct ModuleDescriptor {
    pub name: &'static str,
    pub info: ModuleInfo
}

pub static MODULE_LIST: Spinlock<List<ModuleDescriptor, FixedAllocator<ListNode<ModuleDescriptor>, {Region2 as usize}>>> = Spinlock::new(List::new());

pub fn early_init() {
    let kernel_base_address;
    let kernel_total_size;
    {
        let mut mod_cb = MODULE_LIST.lock();
        let info = BOOT_INFO.get().unwrap().lock();
        kernel_base_address = info.kernel_desc.base;  
        kernel_total_size = info.kernel_desc.total_size; 
        mod_cb.add_node(ModuleDescriptor { name: env!("CARGO_PKG_NAME"), info: info.kernel_desc}).unwrap();
        
        // Map the kernel and auxiliary tables onto upper half
        let mut remap_list = REMAP_LIST.lock();
        remap_list.add_node(RemapEntry {
            value: MemoryRegion {
                base_address: info.kernel_desc.base,
                size: info.kernel_desc.total_size
            },
            map_type: OffsetMapped(|kern_base| {
                let mut mod_glob = MODULE_LIST.lock();
                let mod_cb = mod_glob.first_mut().unwrap();
                let offset = kern_base as isize - mod_cb.info.base as isize;
                let add_offset = |a: usize| {
                    (a as isize + offset) as usize
                };
                
                mod_cb.info.base = kern_base;
                mod_cb.info.entry = add_offset(mod_cb.info.entry);

                let update_array_rgn = |array_rgn: &mut ArrayTable| {
                    let entries = unsafe {
                        core::slice::from_raw_parts_mut(array_rgn.start as *mut MemoryRegion, array_rgn.size / array_rgn.entry_size)
                    };

                    // Update all the entries in the array with new values
                    entries.iter_mut().for_each(|reg| {
                        reg.base_address = add_offset(reg.base_address);
                    });
                    
                    array_rgn.start = add_offset(array_rgn.start);
                };
                
                if let Some(val) = &mut mod_cb.info.sym_tab {
                    val.start = add_offset(val.start);
                }
                if let Some(val) = &mut mod_cb.info.dyn_tab {
                    val.start = add_offset(val.start);
                }
                if let Some(val) = &mut mod_cb.info.dyn_shn {
                    val.start = add_offset(val.start);
                }
                if let Some(val) = &mut mod_cb.info.rlc_shn {
                    update_array_rgn(val);
                }

                if let Some(val) = &mut mod_cb.info.sym_str {
                    val.base_address = add_offset(val.base_address);
                }

                if let Some(val) = &mut mod_cb.info.dyn_str {
                    val.base_address = add_offset(val.base_address);
                }

                debug!("Updated kernel module info = {:?}", mod_cb.info);
            })
        }).unwrap();
    }

    // Temporarily id map the kernel
    // This is to help with the address space transition
    // We need to manually break up the memory here 
    // This is required since otherwise the kernel's memory will overlap the heap memory (which is also part of kernel binary) and 
    // the memory manager will deny the heap to identity map it's memory since kernel has already mapped over that region
    mem::map_kernel_memory(kernel_base_address, kernel_total_size);
}


pub fn complete_handoff() {
    let kernel_base;
    let total_size;
    let load_base;
    
    info!("Reapplying relocations to switch to new address space");
    {
        let mod_list = MODULE_LIST.lock();
        let mod_cb = mod_list.first().unwrap();
        let boot_info = BOOT_INFO.get().unwrap().lock();
        if mod_cb.info.rlc_shn.is_none() {
            return;
        }

        let rlc_tab_desc = mod_cb.info.rlc_shn.unwrap(); 
        let reloc_sections = unsafe {
            core::slice::from_raw_parts(rlc_tab_desc.start as *const MemoryRegion, rlc_tab_desc.size / rlc_tab_desc.entry_size)
        };

        kernel_base = boot_info.kernel_desc.base;
        total_size = boot_info.kernel_desc.total_size;
        load_base = mod_cb.info.base;
        
        let info = |bitmap: u64| {
            (bitmap & 0xffffffff) as u32
        };

        for rlc_shn in reloc_sections {
            let num_rel_entries = rlc_shn.size / core::mem::size_of::<Elf64Rela>();
            let entries = unsafe {
                core::slice::from_raw_parts(rlc_shn.base_address as *const Elf64Rela, num_rel_entries)
            };
            
            for entry in entries {
                match info(entry.r_info) {
                    // Unlike the bootloader, here we will first read the value at the given offset
                    // and calculate the addend from that value instead of applying the addend in the reloc entry
                    // This is apparently needed as some entries can be switched to different values due to a library's
                    // internal routine, but the change won't be reflected in reloc table which causes issue if we ignore it
                    // For example, set_logger(&LOGGER) from log crate has this issue
                    R_X86_64_RELATIVE => {
                        let address = load_base + entry.r_offset as usize;
                        let addend = unsafe {
                            *(address as *mut u64) as usize
                        } - kernel_base;
                        unsafe {
                            *(address as *mut u64) = (load_base + addend) as u64;
                        }
                    },
                    _ => {} 
                }
            }

        }
    }

    mem::unmap_kernel_memory(kernel_base, total_size);
    info!("Handoff procedure completed");
}