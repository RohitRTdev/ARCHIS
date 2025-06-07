use num_traits::{Unsigned, PrimInt};

pub fn ceil_div<T: Unsigned + PrimInt>(a: T, b: T) -> T {
    (a + b - T::one()) / b
}

extern "Rust" {
    fn standard_logger<'a>() -> &'a mut dyn ::core::fmt::Write;
}

pub fn fetch_logger<'a>() -> &'a mut dyn ::core::fmt::Write {
    unsafe {
        standard_logger()
    }
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

#[macro_export]
macro_rules! println {
    () => {
        #[cfg(test)]
        {
            ::std::println!();
        }
        #[cfg(not(test))]
        {
            $crate::fetch_logger().write_fmt(::core::format_args!("\n")).unwrap();
        }
    };
    ($($arg:tt)*) => {
        #[cfg(test)]
        {
            ::std::println!($($arg)*);
        }
        #[cfg(not(test))]
        { 
            $crate::fetch_logger()
                        .write_fmt(::core::format_args!($($arg)*))
                        .and_then(|_| $crate::fetch_logger().write_str("\n"))
                        .unwrap();
        }
    };
}

