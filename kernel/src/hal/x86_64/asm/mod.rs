#[cfg(not(test))]
mod real;

#[cfg(not(test))]
pub use real::*;

mod int_stub;

pub use int_stub::*;

#[cfg(test)]
mod stub;

#[cfg(test)]
pub use stub::*;
