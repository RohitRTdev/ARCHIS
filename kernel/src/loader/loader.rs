use alloc::sync::{Arc, Weak};
use crate::KERNEL_PATH;
use crate::fs::open;
use crate::infra::disable_preloader_phase;
use crate::loader::module::ModuleDescriptor;
use crate::mem::PoolAllocatorGlobal;
use crate::sched::Handle::ImgHandle;
use crate::sched::add_new_handle;
use crate::sync::Spinlock;
use crate::ds::{List, DynList};
use super::module;

pub static KERNEL_MODULES: Spinlock<DynList<Weak<Spinlock<ModuleDescriptor>, PoolAllocatorGlobal>>> = Spinlock::new(List::new());

pub type LoadedImage = Arc<Spinlock<ModuleDescriptor>, PoolAllocatorGlobal>;

pub fn init() {
    let mut kernel_img = module::ARIS.get().unwrap().lock().clone();
    kernel_img.file_handle = Some(
        open(KERNEL_PATH).expect("Failed to open kernel image!")
    );

    let loaded_img = Arc::new_in(
        Spinlock::new(
            kernel_img
        ),
        PoolAllocatorGlobal
    );

    let downgraded_ref = Arc::downgrade(&loaded_img);

    add_new_handle(ImgHandle(loaded_img));
    KERNEL_MODULES.lock().add_node(downgraded_ref)
    .expect("Failed to add kernel image module to Loaded images registry!");

    disable_preloader_phase();
}