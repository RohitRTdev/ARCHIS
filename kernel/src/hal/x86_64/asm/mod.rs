#[cfg(not(test))]
mod real;

#[cfg(not(test))]
pub use real::*;


#[cfg(test)]
mod stub;

#[cfg(test)]
pub use stub::*;
