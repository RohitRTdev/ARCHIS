use common::{MemoryRegion, ModuleInfo};
use crate::{RemapEntry, BOOT_INFO, REMAP_LIST};
use crate::ds::{List, ListNode};
use crate::sync::Spinlock;
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
        is_identity_mapped: false
    }).unwrap();
}
