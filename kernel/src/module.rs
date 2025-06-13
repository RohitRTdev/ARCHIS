use common::ModuleInfo;
use crate::BOOT_INFO;
use crate::ds::{List, ListNode};
use crate::sync::Spinlock;
use crate::mem::*;

pub struct ModuleDescriptor {
    pub name: &'static str,
    pub info: ModuleInfo
}

pub static MODULE_LIST: Spinlock<List<ModuleDescriptor, FixedAllocator<ListNode<ModuleDescriptor>, {Regions::Region1 as usize}>>> = Spinlock::new(List::new());

pub fn early_init() {
    let mut mod_cb = MODULE_LIST.lock();
    let info = BOOT_INFO.get().unwrap().lock();
    
    mod_cb.add_node(ModuleDescriptor { name: env!("CARGO_PKG_NAME"), info: info.kernel_desc});
}

