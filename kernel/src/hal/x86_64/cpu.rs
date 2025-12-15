use core::sync::atomic::{AtomicBool, Ordering};
use crate::ds::*;
use crate::sync::Spinlock;
use super::lapic::*;
use kernel_intf::info;

struct Cpu {
    apic_id: usize,
    logical_id: usize,
    is_bsp: bool   
}

static PRE_INIT_PHASE: AtomicBool = AtomicBool::new(true);
static CPU_LIST: Spinlock<DynList<Cpu>> = Spinlock::new(List::new());

pub fn get_core() -> usize {
    // LAPIC is not initialized yet
    if PRE_INIT_PHASE.load(Ordering::Acquire) {
        return 0;
    }

    let apic_id = get_lapic_id();   

    CPU_LIST.lock().iter().find(|cb| cb.apic_id == apic_id).unwrap().logical_id
}

pub fn register_cpu(apic_id: usize, logical_id: usize) {
    CPU_LIST.lock().add_node(Cpu { apic_id, logical_id, is_bsp: logical_id == 0 }).unwrap();

    info!("Register cpu: {} with apic_id:{}", logical_id, apic_id);

    PRE_INIT_PHASE.store(false, Ordering::Release);
}

pub fn get_bsp_lapic_id() -> usize {
    assert!(PRE_INIT_PHASE.load(Ordering::Acquire) == false);

    CPU_LIST.lock().iter().find(|cb| cb.logical_id == 0).expect("bsp_lapic_id not found!").apic_id
}
