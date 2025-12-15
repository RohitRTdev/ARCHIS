#![no_std]

mod log;
pub use log::*;
use core::fmt;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KError {
    Success,
    InvalidArgument,
    OutOfMemory
}

impl<T> From<Result<T, KError>> for KError {
    fn from(e: Result<T, KError>) -> Self {
        e.err().unwrap_or(KError::Success)
    }
}


impl fmt::Display for KError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let description = match self {
            KError::InvalidArgument => "Invalid argument",
            KError::OutOfMemory => "Out of memory",
            KError::Success => "Success"
        };
        write!(f, "{}", description)
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RtcTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    pub month: u8,
    pub year: u8
}

impl fmt::Display for RtcTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02}/{:02}/{:02}:{:02}-{:02}-{:02}",
            self.day, self.month, self.year, self.hour, self.minute, self.second
        )
    }
}

#[repr(C)]
pub struct Lock {
    pub lock: u64,
    pub int_status: bool
}

extern "C" {
    pub fn create_spinlock(lock: &mut Lock);
    pub fn acquire_spinlock(lock: &mut Lock);
    pub fn release_spinlock(lock: &mut Lock);
    pub fn clear_screen();
    pub fn read_rtc() -> RtcTime;
    pub fn read_timestamp() -> usize;
    pub fn serial_print_ffi(s: *const u8, len: usize);
    pub fn map_memory_ffi(phys_addr: usize, phys_addr: usize, size: usize, flags: u8) -> KError;
    pub fn unmap_memory_ffi(virt_addr: *mut u8, size: usize) -> KError; 
    pub fn allocate_memory_ffi(size: usize, align: usize, flags: u8) -> KError;
    pub fn deallocate_memory_ffi(addr: *mut u8, size: usize, align: usize, flags: u8) -> KError;
}