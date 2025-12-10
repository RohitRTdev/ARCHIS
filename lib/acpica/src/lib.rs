#![no_std]

mod osl;
mod types;
mod table;

pub use table::*;
use kernel_intf::info;
pub use types::*;

pub fn init() {
    unsafe {
        info!("Initializing ACPI subsystem");
        let status = AcpiInitializeSubsystem();
        assert_eq!(status, AE_OK);
        
        info!("Initializing ACPI tables");
        let status = AcpiInitializeTables(core::ptr::null_mut(), 16, 1);
        assert_eq!(status, AE_OK);

        info!("Loading ACPI tables");
        let status = AcpiLoadTables();
        assert_eq!(status, AE_OK);
        
        info!("Enabling ACPI Subsystem");
        let status = AcpiEnableSubsystem(0);
        assert_eq!(status, AE_OK);

        info!("Initializing ACPI objects");
        let status = AcpiInitializeObjects(0);
        assert_eq!(status, AE_OK);
    }
}