use common::{MemoryRegion, ModuleInfo};
use crate::{RemapEntry, RemapType::*, BOOT_INFO, REMAP_LIST};
use crate::ds::{List, ListNode};
use crate::sync::Spinlock;
use crate::logger::debug;
use crate::mem::{FixedAllocator, Regions::*};

pub struct ModuleDescriptor {
    pub name: &'static str,
    pub info: ModuleInfo
}

pub static MODULE_LIST: Spinlock<List<ModuleDescriptor, FixedAllocator<ListNode<ModuleDescriptor>, {Region2 as usize}>>> = Spinlock::new(List::new());

pub fn early_init() {
    let mut mod_cb = MODULE_LIST.lock();
    let info = BOOT_INFO.get().unwrap().lock();
    
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
                val.start = add_offset(val.start);
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
