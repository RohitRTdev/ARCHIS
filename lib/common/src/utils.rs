use num_traits::{Unsigned, PrimInt};

pub fn ceil_div<T: Unsigned + PrimInt>(a: T, b: T) -> T {
    (a + b - T::one()) / b
}

#[macro_export]
macro_rules! test_log {
    ($($arg:tt)*) => {
        #[cfg(test)]
        {
            ::std::println!($($arg)*);
        }
    };
}


