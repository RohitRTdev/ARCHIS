use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::{collections::BTreeMap};
use common::{elf::*, ArrayTable, PAGE_SIZE};
use common::{MemoryRegion, ModuleInfo, FileDescriptor};
use crate::{RemapEntry, RemapType::*, BOOT_INFO, REMAP_LIST};
use crate::ds::{FixedList, List};
use crate::sync::Spinlock;
use kernel_intf::{info, debug};
use crate::mem::{self, MapFetchType, Regions::*};

pub struct ModuleDescriptor {
    pub name: &'static str,
    pub info: ModuleInfo
}

pub static MODULE_LIST: Spinlock<FixedList<ModuleDescriptor, {Region2 as usize}>> = Spinlock::new(List::new());


static FILE_INDEX: AtomicUsize = AtomicUsize::new(0);

pub fn early_init() {
    let kernel_base_address;
    let kernel_total_size;
    {
        let mut mod_cb = MODULE_LIST.lock();
        let info = BOOT_INFO.get().unwrap();
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
            }),
            flags: 0
        }).unwrap();

        // Relocate init fs
        let fs_entries = unsafe {
            core::slice::from_raw_parts_mut(info.init_fs.start as *mut FileDescriptor, info.init_fs.size / info.init_fs.entry_size)
        };

        for entry in fs_entries {
            assert!(entry.contents.as_ptr() as usize & (PAGE_SIZE - 1) == 0);
            remap_list.add_node(RemapEntry { 
                value: MemoryRegion { 
                    base_address: entry.contents.as_ptr() as usize,
                    size: entry.contents.len() + entry.name.len()
                },
                map_type: OffsetMapped(|virt_addr| {
                    let info = BOOT_INFO.get().unwrap();
                    let fs_entries = unsafe {
                        core::slice::from_raw_parts_mut(info.init_fs.start as *mut FileDescriptor, info.init_fs.size / info.init_fs.entry_size)
                    };


                    let entry = &mut fs_entries[FILE_INDEX.fetch_add(1, Ordering::Relaxed)]; 
                    entry.contents = unsafe {
                        core::slice::from_raw_parts(virt_addr as *const u8, entry.contents.len())
                    };
                    
                    entry.name = unsafe {
                        let ptr = core::slice::from_raw_parts((virt_addr + entry.contents.len()) as *const u8, entry.name.len());
                        core::str::from_utf8_unchecked(ptr)
                    };

                }),
                flags: 0
            }).unwrap();
        }

        // Identity map the descriptors pointing to the files 
        remap_list.add_node(RemapEntry { 
            value: MemoryRegion { 
                base_address: info.init_fs.start, 
                size: info.init_fs.size
            }, 
            map_type: IdentityMapped,
            flags: 0
        }).unwrap();
    }


    // Temporarily id map the kernel
    // This is to help with the address space transition
    // We need to manually break up the memory here 
    // This is required since otherwise the kernel's memory will overlap the heap memory (which is also part of kernel binary) and 
    // the memory manager will deny the heap to identity map it's memory since kernel has already mapped over that region
    mem::map_kernel_memory(kernel_base_address, kernel_total_size);
}


pub fn complete_handoff() -> (usize, usize) {
    let kernel_base;
    let total_size;
    
    info!("Reapplying relocations to switch to new address space");
    {
        let mut mod_list = MODULE_LIST.lock();
        let mod_cb = mod_list.first_mut().unwrap();
        let boot_info = BOOT_INFO.get().unwrap();
        kernel_base = boot_info.kernel_desc.base;
        total_size = boot_info.kernel_desc.total_size;
        
        if mod_cb.info.rlc_shn.is_none() {
            return (kernel_base, total_size);
        }

        let rlc_tab_desc = mod_cb.info.rlc_shn.unwrap(); 
        let reloc_sections = unsafe {
            core::slice::from_raw_parts(rlc_tab_desc.start as *const MemoryRegion, rlc_tab_desc.size / rlc_tab_desc.entry_size)
        };

        // This is the old unmapped kernel address
        let load_base = mod_cb.info.base;
        let dyn_tab = mod_cb.info.dyn_tab;
        
        let info = |bitmap: u64| {
            (bitmap & 0xffffffff) as u32
        };
        
        let stringizer = |str_idx: usize| {
            use core::ffi::CStr;

            let str_base = unsafe {
                (mod_cb.info.dyn_str.unwrap().base_address as *const u8).add(str_idx)
            };

            unsafe {
                CStr::from_ptr(str_base as *const i8).to_str().unwrap()
            }
        };

        for rlc_shn in reloc_sections {
            let num_rel_entries = rlc_shn.size / core::mem::size_of::<Elf64Rela>();
            let entries = unsafe {
                core::slice::from_raw_parts(rlc_shn.base_address as *const Elf64Rela, num_rel_entries)
            };
            
            for entry in entries {
                let address = load_base + entry.r_offset as usize;
                match info(entry.r_info) {
                    R_X86_64_RELATIVE => {
                        unsafe {
                            *(address as *mut u64) = (load_base + entry.r_addend as usize) as u64;
                        }
                    },
                    R_JUMP_SLOT => {
                        assert!(dyn_tab.is_some());
                        let dyn_entries = unsafe {
                            let tab = dyn_tab.as_ref().unwrap();
                            core::slice::from_raw_parts(tab.start as *const Elf64Sym, tab.size / tab.entry_size)
                        };

                        let sym_idx = (entry.r_info >> 32) as usize;

                        if dyn_entries[sym_idx].st_shndx == SHN_UNDEF {
                            panic!("Could not find definition for symbol: {}", stringizer(dyn_entries[sym_idx].st_name as usize));
                        }

                        let value = load_base + dyn_entries[sym_idx].st_value as usize;

                        unsafe {
                            *(address as *mut u64) = value as u64;
                        }
                    },

                    _ => {} 
                }
            }
        }
        
        // The module name needs to be patched up to new address
        let name_ptr = mem::get_virtual_address(mod_cb.name.as_ptr() as usize, MapFetchType::Kernel).expect("Unable to find virtual address for module name");
        
        mod_cb.name = unsafe {
            let slice = core::slice::from_raw_parts(name_ptr as *const u8, mod_cb.name.len());
            core::str::from_utf8_unchecked(slice)
        }; 
    
        debug!("Module address:{:#X}, mod_name={}", mod_cb.name.as_ptr() as usize, mod_cb.name);

        // Reconstruct init fs as a hashmap. This is done here, since we now have access to heap
        crate::INIT_FS.call_once(|| {
            let boot_info = BOOT_INFO.get().unwrap();
            let fs_entries = unsafe {
                core::slice::from_raw_parts(boot_info.init_fs.start as *const FileDescriptor, boot_info.init_fs.size / boot_info.init_fs.entry_size) 
            };

            let mut map = BTreeMap::new();

            for entry in fs_entries {
                info!("Adding init fs entry:{} with start_addr:{:#X}", entry.name, entry.contents.as_ptr().addr());
                map.insert(entry.name, entry.contents);
            }
            
            map
        });
    
        info!("Init-FS address:{:#X}, num_files={}", crate::INIT_FS.get().unwrap() as *const _ as usize, crate::INIT_FS.get().unwrap().len());
        
        // We have moved the init-fs metadata into kernel binary
        // So we can remove the descriptors we had
        // We can't call mem::deallocate_memory as the physical memory was allocated by blr. So we just unmap instead
        mem::unmap_memory(boot_info.init_fs.start as *mut u8, boot_info.init_fs.size).expect("Could not deallocate init-fs descriptor memory");
    }


    info!("Handoff procedure completed");
    (kernel_base, total_size)
}