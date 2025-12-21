use common::{en_flag, ptr_to_usize};
use crate::{cpu, hal::enable_interrupts, sync::{Once, Spinlock}};
use kernel_intf::{debug, info};
use super::{asm, MAX_INTERRUPT_VECTORS, handlers, lapic, timer};

const KERNEL_CODE_SELECTOR: usize = 0x8;

struct GDT;
struct TSSDescriptor;
struct IDTDescriptor;

impl GDT {
    const L: u64 = 1 << 53;
    const P: u64 = 1 << 47;
    const DPL_SHIFT: u64 = 45;
    const CODE: u64 = 0x1A << 40;
    const DATA: u64 = 0x12 << 40;

    const fn new(code_segment: bool, long_mode: bool, present: bool, privilege: u64) -> u64 {
        en_flag!(long_mode, Self::L) | en_flag!(present, Self::P) | (privilege << Self::DPL_SHIFT) | 
        if code_segment {
            Self::CODE
        } else {
            Self::DATA
        }
    }
}

impl TSSDescriptor {
    const TSS_TYPE: u64 = 0x9;
    const TYPE_SHIFT: u64 = 40;
    const P: u64 = 1 << 47; 
    const SEG_UPPER_SHIFT: u64 = 48 - 16;
    const SEG_UPPER_MASK: u64 = 0xF << 16;
    const SEG_LOWER_MASK: u64 = 0xFFFF;
    const ADDRESS_LOWER_MASK: u64 = 0xFFFFFF;
    const ADDRESS_LOWER_SHIFT: u64 = 16;
    const ADDRESS_UPPER_MASK: u64 = 0xFFFFFFFF << 32;
    const ADDRESS_UPPER_SHIFT: u64 = 32;
    const ADDRESS_MIDDLE_MASK: u64 = 0xFF << 24;
    const ADDRESS_MIDDLE_SHIFT: u64 = 56 - 24;

    fn new(seg_limit: u64, base_address: u64) -> [u64; 2] {
        [(Self::TSS_TYPE << Self::TYPE_SHIFT) | Self::P | (seg_limit & Self::SEG_LOWER_MASK) | ((seg_limit & Self::SEG_UPPER_MASK) << Self::SEG_UPPER_SHIFT)
        | ((base_address & Self::ADDRESS_LOWER_MASK) << Self::ADDRESS_LOWER_SHIFT) | ((base_address & Self::ADDRESS_MIDDLE_MASK) << Self::ADDRESS_MIDDLE_SHIFT), 
        (base_address & Self::ADDRESS_UPPER_MASK) >> Self::ADDRESS_UPPER_SHIFT] 
    }
}

#[repr(C, packed)]
pub struct TaskStateSegment {
    _reserved1: u32,
    rsp0: u64,       
    rsp1: u64,       
    rsp2: u64,       
    _reserved2: u64,
    ist: [u64; 7],   
    _reserved3: u64,
    _reserved4: u16,
    iomap_base: u16
}

impl TaskStateSegment {
    pub const fn new(stack_address: u64, good_stack: u64) -> Self {
        let mut task = Self {
            _reserved1: 0,
            rsp0: stack_address,
            rsp1: 0,
            rsp2: 0,
            _reserved2: 0,
            ist: [0; 7],
            _reserved3: 0,
            _reserved4: 0,
            // This along with the limit in TSSDescriptor effectively disables IOPB permission bitmap
            iomap_base: core::mem::size_of::<Self>() as u16,
        };

        task.ist[0] = good_stack;
        task
    }
}

impl IDTDescriptor {
    const TARGET_ADDR_LOW_MASK: u64 = 0xFFFF;
    const TARGET_ADDR_HIGH_MASK: u64 = 0xFFFFFFFF << 32;
    const TARGET_ADDR_HIGH_SHIFT: u64 = 32;
    const TARGET_ADDR_MIDDLE_MASK: u64 = 0xFFFF << 16;
    const TARGET_ADDR_MIDDLE_SHIFT: u64 = 48 - 16;
    const P: u64 = 1 << 47;
    const DPL: u64 = 0x3 << 45;
    const TYPE_IDT: u64 = 0xE;
    const TYPE_SHIFT: u64 = 40;
    const SELECTOR_SHIFT: u64 = 16;
    const IST0_SHIFT: u64 = 32;

    fn new(selector: u64, handler_address: u64, set_ist: bool) -> [u64; 2] {
        [Self::P | Self::DPL | (Self::TYPE_IDT << Self::TYPE_SHIFT) | ((if set_ist {1} else {0}) << Self::IST0_SHIFT) |
        (selector << Self::SELECTOR_SHIFT) | (handler_address & Self::TARGET_ADDR_LOW_MASK) |
        ((handler_address & Self::TARGET_ADDR_MIDDLE_MASK) << Self::TARGET_ADDR_MIDDLE_SHIFT),
        (handler_address & Self::TARGET_ADDR_HIGH_MASK) >> Self::TARGET_ADDR_HIGH_SHIFT]
    }
}

#[repr(C, packed)]
#[derive(Debug)]
struct TableLayout {
    limit: u16,
    base_address: u64   
}

#[repr(align(8))]
struct TableData {
    gdt_array: [u64; 7],
    gdt_layout: TableLayout,
    idt: [u64; MAX_INTERRUPT_VECTORS * 2],
    idt_layout: TableLayout
}

static CPU_TABLE_DATA: Spinlock<TableData> = Spinlock::new (TableData { gdt_array: [0; 7],
    gdt_layout: TableLayout { limit: 0, base_address: 0 }, idt: [0; MAX_INTERRUPT_VECTORS * 2], 
    idt_layout: TableLayout { limit: 0, base_address: 0 }});

static CPU_TSS: Once<TaskStateSegment> = Once::new();

#[no_mangle]
pub extern "C" fn kern_addr_space_start() -> ! {
    info!("Switched to new address space");
    crate::cpu::set_panic_base(cpu::get_current_stack_base());
    crate::module::complete_handoff();

    info!("CPU-0 stack address:{:#X}", cpu::get_current_stack_base());

    CPU_TSS.call_once(|| {
        TaskStateSegment::new(cpu::get_current_stack_base() as u64,
    cpu::get_current_good_stack_base() as u64) 
    });

    let tss_base = CPU_TSS.get().unwrap() as *const _ as u64;
    let tss_desc =  TSSDescriptor::new(tss_base + size_of::<TaskStateSegment>() as u64 - 1, tss_base);  

    {
        let mut cpu_table = CPU_TABLE_DATA.lock();
        cpu_table.gdt_array = [
                    // Current layout
                    // Null segment + Kernel code + Kernel data + User data + User code
                    // This layout is required for syscall/sysret to work
                    // With this layout the segment selectors are as follows
                    // Kernel code -> CS=0x8, Kernel data -> SS=0x10,
                    // User code -> CS=0x23, User data -> SS=0x1B
                    // TSS=0x28
                    
                    GDT::new(false, false, false, 0),
                    GDT::new(true, true, true, 0),
                    GDT::new(false, false, true, 0),
                    GDT::new(false, false, true, 3),
                    GDT::new(true, true, true, 3),
                    tss_desc[0],
                    tss_desc[1]
                ];
        
        cpu_table.gdt_layout.base_address = cpu_table.gdt_array.as_ptr() as u64;     
        cpu_table.gdt_layout.limit = (7 * size_of::<u64>() - 1) as u16;

        debug!("Interrupt stub address for vector 0 -> {:#X}", asm::IDT_TABLE[0] as u64);

        for vector in 0..MAX_INTERRUPT_VECTORS {
            let idt_desc = IDTDescriptor::new(KERNEL_CODE_SELECTOR as u64, asm::IDT_TABLE[vector] as u64,
            vector == super::DOUBLE_FAULT_VECTOR || vector == super::NMI_FAULT_VECTOR);
            cpu_table.idt[vector * 2] = idt_desc[0];
            cpu_table.idt[vector * 2 + 1] = idt_desc[1]; 
        }

        cpu_table.idt_layout.base_address = cpu_table.idt.as_ptr() as u64;     
        cpu_table.idt_layout.limit = (MAX_INTERRUPT_VECTORS * 2 * size_of::<u64>() - 1) as u16;
        
        debug!("Setting gdt layout={:?}", cpu_table.gdt_layout);
        debug!("Setting idt layout={:?}", cpu_table.idt_layout);

        unsafe {
            asm::setup_table(ptr_to_usize(&cpu_table.gdt_layout) as u64, ptr_to_usize(&cpu_table.idt_layout) as u64);
        }
    }

    lapic::init();
    timer::init();
    handlers::init();
    
    enable_interrupts(true);
    crate::kern_main();
} 