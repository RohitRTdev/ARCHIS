use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KError {
    InvalidArgument,
    OutOfMemory
}

impl fmt::Display for KError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let description = match self {
            KError::InvalidArgument => "Invalid argument",
            KError::OutOfMemory => "Out of memory",
        };
        write!(f, "{}", description)
    }
}
