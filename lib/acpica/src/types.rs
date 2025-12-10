#![allow(non_camel_case_types)]

use core::ffi::{c_void, c_char};

pub type ACPI_STATUS = u32;
pub type ACPI_PHYSICAL_ADDRESS = u64;
pub type ACPI_THREAD_ID = u64;
pub type ACPI_SIZE = usize;
pub type ACPI_SEMAPHORE = *mut c_void;
pub type ACPI_SPINLOCK = *mut c_void;
pub type ACPI_STRING = *const c_char;
pub type ACPI_OSD_HANDLER = extern "C" fn(*mut c_void);
pub type ACPI_OSD_EXEC_CALLBACK = extern "C" fn(*mut c_void);

#[repr(C)]
pub struct ACPI_PREDEFINED_NAMES {
    name: *const c_char,
    type_acpi: u8,
    val: *mut c_char
}

#[repr(C, packed)]
pub struct ACPI_TABLE_HEADER {
    signature: [u8; ACPI_NAMESEG_SIZE],      
    length: u32,                            
    revision: u8, 
    checksum: u8,  
    oem_id: [u8; ACPI_OEM_ID_SIZE],
    oem_table_id: [u8; ACPI_OEM_TABLE_ID_SIZE],
    oem_rev: u32,
    asl_compiler_id: [u8; ACPI_NAMESEG_SIZE],
    asl_compiler_rev: u32
}

#[repr(C)]
pub struct ACPI_PCI_ID {
    segment: u16,
    bus: u16,
    device: u16,
    function: u16
}

#[repr(C, packed)]
pub struct ACPI_TABLE_HPET {
    pub header: ACPI_TABLE_HEADER,   // Standard ACPI table header
    pub event_timer_block_id: u32,   // Hardware ID of the timer block
    pub address: ACPI_GENERIC_ADDRESS, // Base address of HPET registers
    pub hpet_number: u8,             // HPET sequence number
    pub min_tick: u16,               // Minimum clock tick in periodic mode
    pub flags: u8                   // Flags (bit 0: LegacyReplacement)
}

#[repr(C, packed)]
pub struct ACPI_GENERIC_ADDRESS {
    pub space_id: u8,
    pub bit_width: u8,
    pub bit_offset: u8,
    pub access_width: u8
}

extern "C" {
    pub fn AcpiInitializeSubsystem() -> ACPI_STATUS;
    pub fn AcpiInitializeTables(initial_storage: *mut c_void, initial_table_count: u32, allow_resize: u8) -> ACPI_STATUS;
    pub fn AcpiLoadTables() -> ACPI_STATUS;
    pub fn AcpiEnableSubsystem(flags: u32) -> ACPI_STATUS;
    pub fn AcpiInitializeObjects(flags: u32) -> ACPI_STATUS;
}

pub const AE_OK: ACPI_STATUS = 0x0000_0000;
pub const AE_ERROR: ACPI_STATUS = 0x0000_0001;
pub const ACPI_NAMESEG_SIZE: usize = 4;
pub const ACPI_OEM_ID_SIZE: usize = 6;
pub const ACPI_OEM_TABLE_ID_SIZE: usize = 8;

