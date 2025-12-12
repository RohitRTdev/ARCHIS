use core::ptr;
use core::ffi::{c_char, c_void, CStr};
use kernel_intf::info;

use crate::types::*;

#[no_mangle]
extern "C" fn AcpiOsInitialize() -> ACPI_STATUS {
    info!("ACPICA initialize");
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsTerminate() -> ACPI_STATUS {
    info!("ACPICA terminate");
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsGetRootPointer() -> ACPI_PHYSICAL_ADDRESS {
    1
}

#[no_mangle]
extern "C" fn AcpiOsPredefinedOverride(predefined_obj: *const ACPI_PREDEFINED_NAMES, new_value: *mut ACPI_STRING) -> ACPI_STATUS {
    if predefined_obj.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *new_value = ptr::null();
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsTableOverride (existing_table: *mut AcpiTableHeader, new_table: *mut *const AcpiTableHeader) -> ACPI_STATUS {
    if existing_table.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *new_table = ptr::null();
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsPhysicalTableOverride (existing_table: *const AcpiTableHeader, new_address: *mut ACPI_PHYSICAL_ADDRESS, 
    new_table_length: *mut ACPI_SIZE) -> ACPI_STATUS {
    if existing_table.is_null() {
        return AE_ERROR;
    }   

    unsafe {
        *new_address = 0;
        *new_table_length = 0;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsCreateCache (cache_name: *const c_char, object_size: u16, max_depth: u16, return_cache: *mut *mut c_void) -> ACPI_STATUS {
    let s = unsafe {
        CStr::from_ptr(cache_name as *const i8).to_str().unwrap()
    };
    
    info!("ACPICA: Create cache:{}, object_size:{}", s, object_size);
    if cache_name.is_null() || return_cache.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *return_cache = ptr::null_mut();
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsDeleteCache (cache: *mut c_void) -> ACPI_STATUS {
    if cache.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsPurgeCache (cache: *mut c_void) -> ACPI_STATUS {
    if cache.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsAcquireObject (cache: *mut c_void) -> *mut c_void {
    info!("ACPICA: Acquire object:{:#X}", cache as usize);
    if cache.is_null() {
        return ptr::null_mut();
    }

    ptr::null_mut()
}

#[no_mangle]
extern "C" fn AcpiOsReleaseObject (cache: *mut c_void, object: *const c_void) -> ACPI_STATUS {
    if cache.is_null() || object.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsMapMemory (phys_addr: ACPI_PHYSICAL_ADDRESS, length: ACPI_SIZE) -> *mut c_void {
    if length == 0 {
        return ptr::null_mut();
    }

    ptr::null_mut()
}

#[no_mangle]
extern "C" fn AcpiOsUnmapMemory (virt_addr: *const c_void, length: ACPI_SIZE) {
}


#[no_mangle]
extern "C" fn AcpiOsGetPhysicalAddress (virt_addr: *const c_void, phys_addr: *mut ACPI_PHYSICAL_ADDRESS) -> ACPI_STATUS {
    if virt_addr.is_null() || phys_addr.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *phys_addr = 0;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsAllocate(size: ACPI_SIZE) -> *mut c_void {
    ptr::null_mut()
}

#[no_mangle]
extern "C" fn AcpiOsFree(ptr: *mut c_void) {
}

#[no_mangle]
extern "C" fn AcpiOsReadable (memory: *const c_void, length: ACPI_SIZE) -> u8 {
    if memory.is_null() || length == 0 {
        return 0;
    }

    1
}

#[no_mangle]
extern "C" fn AcpiOsWritable (memory: *const c_void, length: ACPI_SIZE) -> u8 {
    if memory.is_null() || length == 0 {
        return 0;
    }

    1
}

#[no_mangle]
extern "C" fn AcpiOsGetThreadId() -> ACPI_THREAD_ID {
    1
}

#[no_mangle]
extern "C" fn AcpiOsExecute(_type: u32, func: ACPI_OSD_EXEC_CALLBACK, ctx: *mut c_void) -> ACPI_STATUS {
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsSleep(milliseconds: u64) -> ACPI_STATUS {
    if milliseconds == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsStall(microseconds: u64) -> ACPI_STATUS {
    if microseconds == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsWaitEventsComplete() 
{}

#[no_mangle]
extern "C" fn AcpiOsCreateMutex(out_handle: *mut *mut c_void) -> ACPI_STATUS {
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsDeleteMutex(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsAcquireMutex(handle: *mut c_void, timeout: u16) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsReleaseMutex(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsCreateSemaphore(max_units: u32, initial_units: u32, out_handle: *mut *mut c_void) -> ACPI_STATUS {
    if out_handle.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *out_handle = ptr::null_mut();
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsDeleteSemaphore(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsWaitSemaphore(handle: *mut c_void, units: u32, timeout: u16) -> ACPI_STATUS {
    if handle.is_null() || units == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsSignalSemaphore(handle: *mut c_void, units: u32) -> ACPI_STATUS {
    if handle.is_null() || units == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsCreateLock(out_handle: *mut *mut c_void) -> ACPI_STATUS {
    if out_handle.is_null() {
        return AE_ERROR;
    }

    unsafe {
        *out_handle = ptr::null_mut();
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsDeleteLock(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsAcquireLock(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsReleaseLock(handle: *mut c_void) -> ACPI_STATUS {
    if handle.is_null() {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsInstallInterruptHandler(interrupt_number: u32, handler: ACPI_OSD_HANDLER, context: *mut c_void) -> ACPI_STATUS {
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsRemoveInterruptHandler(interrupt_number: u32, handler: ACPI_OSD_HANDLER) -> ACPI_STATUS {
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsReadMemory(phys_addr: ACPI_PHYSICAL_ADDRESS, value: *mut u64, width: u32) -> ACPI_STATUS {
    if value.is_null() || width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsWriteMemory(phys_addr: ACPI_PHYSICAL_ADDRESS, value: u64, width: u32) -> ACPI_STATUS {
    if width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsReadPort(port: u16, value: *mut u32, width: u32) -> ACPI_STATUS {
    if value.is_null() || width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsWritePort(port: u16, value: u32, width: u32) -> ACPI_STATUS {
    if width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsReadPciConfiguration(handle: AcpiPciId, reg: u32, value: *mut u64, width: u32) -> ACPI_STATUS {
    if value.is_null() || width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsWritePciConfiguration(handle: AcpiPciId, reg: u32, value: u64, width: u32) -> ACPI_STATUS {
    if width == 0 {
        return AE_ERROR;
    }

    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsPrintStr(s: *const u8) {
    if s.is_null() {
        return;
    }

    let s = unsafe {
        CStr::from_ptr(s as *const i8).to_str().unwrap()
    };

    info!("ACPICA: {}", s);     
}

#[no_mangle]
extern "C" fn AcpiOsRedirectOutput(_file: *mut c_void) -> ACPI_STATUS {
    AE_OK
}

#[no_mangle]
extern "C" fn AcpiOsGetTimer() -> u64 {
    0
}

#[no_mangle]
extern "C" fn AcpiOsSignal(function: u32, info: *const c_void) -> ACPI_STATUS {
    AE_OK
}

