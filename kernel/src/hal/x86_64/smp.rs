use crate::{hal::get_bsp_lapic_id, mem::{PageDescriptor, map_memory}};
use core::sync::atomic::{AtomicBool, Ordering};
use acpica::AcpiTableMadt;
use crate::BOOT_INFO;
use crate::mem::PHY_MEM_CB;
use crate::ds::*;
use crate::sync::Spinlock;
use crate::cpu;
use common::madt::*;
use kernel_intf::{debug, info};
use alloc::alloc::Layout;
use common::PAGE_SIZE;
use super::page_mapper;
use super::lapic;
use super::timer;

#[derive(Debug)]
struct Lapic {
    id: usize,
    uid: usize,
    is_x2apic: bool
}

#[derive(Debug)]
struct Nmi {
    uid: usize,
    pin: u8,
    is_active_high: bool,
    is_edge_triggered: bool
}

static LAPIC_LIST: Spinlock<DynList<Lapic>> = Spinlock::new(List::new());
static NMI_LIST: Spinlock<DynList<Nmi>> = Spinlock::new(List::new());

static AP_INIT_COMPLETE: AtomicBool = AtomicBool::new(false);

static AP_TRAMPOLINE: &[u8] = include_bytes!(env!("TRAMPOLINE_BIN"));

include!("asm/trampoline_offsets.rs");

fn parse_madt(madt: &AcpiTableMadt) {
    let madt_start = madt as *const _ as usize;
    let madt_len = madt.header.length as usize;

    let entries_start = madt_start + size_of::<AcpiTableMadt>();
    let entries_end = madt_start + madt_len;

    let mut ptr = entries_start;

    while ptr < entries_end {
        let hdr = unsafe {
            &*(ptr as *const MadtEntryHeader) 
        };

        // Sanity check 
        if (hdr.length as usize) < size_of::<MadtEntryHeader>() {
            break;
        }

        match hdr.entry_type {
            XLAPIC => {
                let entry = unsafe {
                    &*(ptr as *const MadtLapic)
                };

                // LAPIC is enabled
                if entry.flags & 0x1 != 0 {
                    let lapic = Lapic {
                        id: entry.apic_id as usize,
                        uid: entry.uid as usize,
                        is_x2apic: false
                    };

                    debug!("Adding LAPIC entry {:?}", lapic);

                    // We're just going to assume here for now that firmware 
                    // won't provide the same lapic as both xlapic and x2lapic structure
                    // This is a fair assumption (ACPI spec mentions it)
                    LAPIC_LIST.lock().add_node(lapic).expect("Couldn't store lapic info in list!");
                }
            },
            X2LAPIC => {
                let entry = unsafe {
                    &*(ptr as *const MadtX2Lapic)
                };

                // LAPIC is enabled
                if entry.flags & 0x1 != 0 {
                    let lapic = Lapic {
                        id: entry.apic_id as usize,
                        uid: entry.uid as usize,
                        is_x2apic: true
                    };
                    
                    debug!("Adding X2LAPIC entry {:?}", lapic);

                    LAPIC_LIST.lock().add_node(lapic).expect("Couldn't store lapic info in list!");
                }
            }
            XAPIC_NMI => {
                let entry = unsafe {
                    &*(ptr as *const MadtLapicNmi)
                };

                let nmi = Nmi {
                    uid: entry.uid as usize,
                    pin: entry.pin,
                    is_active_high: (entry.flags & 0x3) == 0x1,
                    is_edge_triggered: ((entry.flags >> 2) & 0x3) != 0x3
                };
                    
                debug!("Adding LAPIC NMI entry {:?}", nmi);

                NMI_LIST.lock().add_node(nmi).expect("Couldn't add NMI pin data to NMI list!");
            },
            X2APIC_NMI => {
                let entry = unsafe {
                    &*(ptr as *const MadtX2LapicNmi)
                };

                let nmi = Nmi {
                    uid: entry.uid as usize,
                    pin: entry.pin,
                    is_active_high: (entry.flags & 0x3) == 0x2,
                    is_edge_triggered: ((entry.flags >> 2) & 0x3) != 0x3
                };

                debug!("Adding x2LAPIC NMI entry {:?}", nmi);
                NMI_LIST.lock().add_node(nmi).expect("Couldn't add NMI pin data to NMI list!");
            }
            _ => {
            }
        }

        ptr += hdr.length as usize;
    }
}

// The stack will be fixed per ap, so we won't do it here
unsafe fn patch_trampoline(load_addr: *mut u8, pml4: u32, ap_init: u64) {
    debug!("Patching trampoline with pml4={:#X} and ap_init_address={:#X}", pml4, ap_init);
    
    let gdt = load_addr.add(GDT);
    let gdt_desc = load_addr.add(GDT_DESC);

    (gdt_desc.add(2) as *mut u32).write_unaligned(gdt as u32);
    let pml4_phys = load_addr.add(PML4_PHYS);
    let ap_entry = load_addr.add(AP_ENTRY);
    (pml4_phys as *mut u32).write(pml4);
    (ap_entry as *mut u64).write(ap_init);

    // Apply manual relocation to instructions
    (load_addr.add(_PATCH1 + 4) as *mut u16).write_unaligned(gdt_desc as u16);
    (load_addr.add(_PATCH2 + 1) as *mut u16).write_unaligned((load_addr.addr() + PMODE_ENTRY) as u16);
    (load_addr.add(_PATCH3 + 1) as *mut u32).write_unaligned((load_addr.addr() + PML4_PHYS) as u32);
    (load_addr.add(_PATCH4 + 1) as *mut u32).write_unaligned((load_addr.addr() + LMODE_ENTRY) as u32);
}


#[cfg(feature = "acpi")]
pub fn init() {

    let madt_tab = acpica::fetch_acpi_table::<AcpiTableMadt>(
        BOOT_INFO.get().unwrap().rsdp as *const u8).expect("No MADT ACPI table found!");

    parse_madt(madt_tab);

    info!("Found {} enabled logical cores in system", LAPIC_LIST.lock().get_nodes());

    let tramp_start = AP_TRAMPOLINE.as_ptr().addr();
    let tramp_size = AP_TRAMPOLINE.len(); 

    info!("Trampoline start={:#X}, size={}", tramp_start, tramp_size);

    let ap_start_code = {
        let mut frame_allocator = PHY_MEM_CB.get().unwrap().lock();
        frame_allocator.configure_upper_limit((1 << 20) - 1);
        
        let addr = frame_allocator.allocate(Layout::from_size_align(tramp_size, PAGE_SIZE).unwrap());

        // Switch back to 4GB limit (needed by MP init)
        frame_allocator.configure_upper_limit((1 << 32) - 1);

        addr
    }.expect("Unable to find suitable memory region < 1MB for ap init code!!");

    info!("Trampoline mapped to region: {:#X}", ap_start_code.addr());

    map_memory(ap_start_code as usize, ap_start_code as usize, tramp_size, PageDescriptor::VIRTUAL)
    .expect("Failed to identity map ap trampoline region to kernel address space!");

    // Copy the trampoline to < 1MB region
    unsafe {
        core::ptr::copy_nonoverlapping(
            tramp_start as *const u8,
            ap_start_code,
            tramp_size,
        );
        
        patch_trampoline(ap_start_code, page_mapper::get_kernel_pml4() as u32, ap_init as *const () as u64);
    }
    
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

    let bsp_id = get_bsp_lapic_id();
    for (idx, core) in LAPIC_LIST.lock().iter().enumerate() {
        if core.id == bsp_id {
            continue;
        } 

        cpu::register_cpu();
        let stack_base = cpu::get_worker_stack(idx);

        debug!("Setting stack base {:#X} for core {}", stack_base, idx);
        
        // Each AP gets their own stack. However, due to the way our trampoline is structured
        // we only let one ap run the trampoline at a time
        unsafe {
            (ap_start_code.add(AP_STACK_TOP) as *mut u64)
                .write_volatile(stack_base as u64);
        }
        
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        AP_INIT_COMPLETE.store(false, Ordering::SeqCst);
        let sipi_vector = (ap_start_code.addr() >> 12) as u8;
        debug!("Sending INIT-SIPI-SIPI sequence to core:{} with apic_id:{} at vector: {}", idx, core.id, sipi_vector);

        lapic::send_init_ipi(core.id as u32);
        timer::delay_ns(10_000_000);
        lapic::send_init_deassert(core.id as u32);
        timer::delay_ns(200_000);

        lapic::send_sipi(core.id as u32, sipi_vector);
        timer::delay_ns(200_000);
        lapic::lapic_wait_icr_idle();

        lapic::send_sipi(core.id as u32, sipi_vector);
        timer::delay_ns(200_000);
        lapic::lapic_wait_icr_idle();

        // Wait for core to complete
        while AP_INIT_COMPLETE.load(Ordering::SeqCst) == false {
            core::hint::spin_loop();
        }
    }

    // From this point on, pages can be freely allocated from any range in the physical address space
    PHY_MEM_CB.get().unwrap().lock().disable_upper_limit();
}

#[no_mangle]
extern "C" fn ap_init() -> ! {
    AP_INIT_COMPLETE.store(true, Ordering::SeqCst);

    info!("Starting new core...");
    crate::hal::halt();
}