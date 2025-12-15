use crate::mem::{MapFetchType, PageDescriptor, allocate_memory, get_virtual_address, map_memory};

use super::asm::{rdmsr, wrmsr};
use super::cpu::register_cpu;
use super::handlers::{SPURIOUS_VECTOR, ERROR_VECTOR};
use kernel_intf::info;
use common::PAGE_SIZE;
use core::alloc::Layout;

const APIC_BASE_OFFSET: u32 = 0x1b;
const APIC_ID_OFFSET: u32 = 0x802;
const APIC_EOI_OFFSET: u32 = 0x80b;
const TASK_REG_OFFSET: u32 = 0x808;
const THERMAL_LVT: u32 = 0x833;
const PERF_CNTR_LVT: u32 = 0x834;
const LINT0_LVT: u32 = 0x835;
const LINT1_LVT: u32 = 0x836;
const ERROR_LVT: u32 = 0x837;
const ERROR_STS_OFFSET: u32 = 0x828;
const SPURIOUS_ENTRY_OFFSET: u32 = 0x80f;

static mut X2APIC_ENABLED: bool = false;

fn lapic_mmio_offset(msr: u32) -> usize {
    ((msr - 0x800) << 4) as usize
}

fn lapic_read(offset: u32) -> u64 {
    unsafe {
        if X2APIC_ENABLED {
            rdmsr(offset)
        } else {
            let mmio_base = get_apic_mmio_base();
            core::ptr::read_volatile((mmio_base + lapic_mmio_offset(offset) as usize) as *const u32) as u64
        }
    }
}

fn lapic_write(offset: u32, value: u64) {
    unsafe {
        if X2APIC_ENABLED {
            wrmsr(offset, value);
        } else {
            let mmio_base = get_apic_mmio_base();
            core::ptr::write_volatile((mmio_base + lapic_mmio_offset(offset) as usize) as *mut u32, value as u32);
        }
    }
}

fn get_apic_mmio_base() -> usize {
    let phy_addr = unsafe {
        (rdmsr(APIC_BASE_OFFSET) & 0xfffff000) as usize
    };

    get_virtual_address(phy_addr, MapFetchType::Any)
    .expect("Unable to get APIC base register space virtual address")
}

pub fn enable_x2apic() {
    unsafe {
        X2APIC_ENABLED = true;
    }
}

pub fn init() {
    let apic_base = unsafe {
        rdmsr(APIC_BASE_OFFSET)
    };

    let apic_base_addr = apic_base & 0xfffff000;
    let is_bsp = ((apic_base >> 8) & 0x1) != 0;

    info!("LAPIC base: {:#X}, is_bsp: {}", apic_base & 0xfffff000, is_bsp);

    if unsafe {X2APIC_ENABLED} {
        // Enable APIC + x2APIC mode
        unsafe {
            wrmsr(APIC_BASE_OFFSET, apic_base | (0x3 << 10));
        }
    } else {
        // Legacy xAPIC mode
        unsafe {
            wrmsr(APIC_BASE_OFFSET, apic_base | (0x1 << 11));
        }

        // Map the APIC register space
        // Here, we're making the assumption that every AP will have the same APIC_BASE_ADDRESS as BSP
        if is_bsp {
            let base = allocate_memory(Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap(), 
            PageDescriptor::VIRTUAL | PageDescriptor::NO_ALLOC)
            .expect("Virtual memory allocation failed for APIC register space");

            map_memory(apic_base_addr as usize, base as usize, PAGE_SIZE, PageDescriptor::MMIO)
            .expect("map_memory failed for apic register space");
        }
    }

    // Allow all interrupts
    lapic_write(TASK_REG_OFFSET, 0);

    // Mask THERMAL, PERF, LINT0/1 LVT entries
    for &addr in &[THERMAL_LVT, PERF_CNTR_LVT, LINT0_LVT, LINT1_LVT] {
        let lvt = lapic_read(addr);
        lapic_write(addr, lvt | (1 << 16));
    }

    // Setup the error table vector entry
    lapic_write(ERROR_LVT, (ERROR_VECTOR & 0xff) as u64);

    // Setup spurious vector entry
    lapic_write(SPURIOUS_ENTRY_OFFSET, (0x3 << 8) | (SPURIOUS_VECTOR & 0xff) as u64);

    if is_bsp {
        register_cpu(get_lapic_id(), 0);
    }
}

pub fn get_lapic_id() -> usize {
    lapic_read(APIC_ID_OFFSET) as usize
}

pub fn eoi() {
    lapic_write(APIC_EOI_OFFSET, 0);
}

pub fn get_error() -> u64 {
    // This write is required to get latest error status
    lapic_write(ERROR_STS_OFFSET, 0);
    lapic_read(ERROR_STS_OFFSET)
}
