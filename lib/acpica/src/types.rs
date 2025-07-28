#[allow(non_camel_case_types)]
pub type ACPI_STATUS = u32;

extern "C" {
    pub fn AcpiInitializeSubsystem() -> ACPI_STATUS;
    pub fn AcpiInitializeTables(
        initial_storage: *mut core::ffi::c_void,
        initial_table_count: u32,
        allow_resize: u8,
    ) -> ACPI_STATUS;
    pub fn AcpiLoadTables() -> ACPI_STATUS;
    pub fn AcpiEnableSubsystem(flags: u32) -> ACPI_STATUS;
    pub fn AcpiInitializeObjects(flags: u32) -> ACPI_STATUS;
}

pub const AE_OK: ACPI_STATUS = 0x0000_0000;
pub const AE_ERROR: ACPI_STATUS = 0x0000_0001;