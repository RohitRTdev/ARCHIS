use crate::types::{ACPI_PHYSICAL_ADDRESS, ACPI_SIZE};
use core::ptr;


fn fetch_acpi_table_core(rsdt_ptr: *const u8, signature: &str) -> Option<*const u8> {
    if rsdt_ptr.is_null() {
        return None;
    }

    let signature = signature.as_bytes();
    // RSDT header: first 36 bytes are ACPI_TABLE_HEADER
    // After that, it's an array of u32 physical addresses to other tables
    let header_len = 36;
    let length = unsafe {
        *(rsdt_ptr.add(4) as *const u32) as usize // Total table length
    };
    let entries = (length - header_len) / 4;

    for i in 0..entries {
        let table_addr = unsafe {
            *(rsdt_ptr.add(header_len + i * 4) as *const u32) as usize
        };
        let table_ptr = table_addr as *const u8;
        if table_ptr.is_null() {
            continue;
        }
        // Check signature
        let table_sig = unsafe {
            core::slice::from_raw_parts(table_ptr, 4)
        };
        if table_sig == signature {
            // SAFETY: Caller must ensure the physical memory is mapped and valid for T
            return Some(table_ptr);
        }
    }
    None

}



pub fn fetch_acpi_table<ACPI_TABLE_HPET>(rsdt_ptr: *const u8) -> Option<&'static ACPI_TABLE_HPET> {
    fetch_acpi_table_core(rsdt_ptr, "HPET").and_then(|table_ptr| {
        unsafe {
            Some(&*(table_ptr as *const ACPI_TABLE_HPET))
        }
    })
}