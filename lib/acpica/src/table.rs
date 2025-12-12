use crate::types::{AcpiTable, AcpiTableHeader};

// These are helper table functions that can be used before/after acpica init
fn fetch_acpi_table_core(rsdt_ptr: *const u8, signature: &str) -> Option<*const u8> {
    if rsdt_ptr.is_null() {
        return None;
    }

    let signature = signature.as_bytes();
    // RSDT header: first 36 bytes are AcpiTableHeader
    // After that, it's an array of u32 physical addresses to other tables
    let header_len = size_of::<AcpiTableHeader>();
    let length = unsafe {
        *(rsdt_ptr.add(4) as *const u32) as usize 
    };
    let entries = (length - header_len) / 4;

    for i in 0..entries {
        let table_addr = unsafe {
            *(rsdt_ptr.add(header_len + i * 4) as *const u32) as *const u8
        };

        // Not sure if this can happen, but just a safeguard
        if table_addr.is_null() {
            continue;
        }

        // The first 4 bytes of a table is it's signature
        let table_sig = unsafe {
            core::slice::from_raw_parts(table_addr, 4)
        };

        // We identity map the ACPI tables for now, so nothing more to do here
        if table_sig == signature {
            return Some(table_addr);
        }
    }
    None

}

pub fn fetch_acpi_table<T: AcpiTable>(rsdt_ptr: *const u8) -> Option<&'static T> {
    fetch_acpi_table_core(rsdt_ptr, T::TABLE_NAME).and_then(|table_ptr| {
        unsafe {
            Some(&*(table_ptr as *const T))
        }
    })
}