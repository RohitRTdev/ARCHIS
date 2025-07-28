#![no_std]

mod osl;
mod types;

use types::*;

pub fn init() {
    unsafe {
        let status = AcpiInitializeSubsystem();
        assert_eq!(status, AE_OK);

        let status = AcpiInitializeTables(core::ptr::null_mut(), 16, 1);
        assert_eq!(status, AE_OK);

        let status = AcpiLoadTables();
        assert_eq!(status, AE_OK);

        let status = AcpiEnableSubsystem(0);
        assert_eq!(status, AE_OK);

        let status = AcpiInitializeObjects(0);
        assert_eq!(status, AE_OK);
    }
}