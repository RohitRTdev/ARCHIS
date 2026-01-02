use num_traits::{Unsigned, PrimInt};

#[inline(always)]
pub fn ceil_div<T: Unsigned + PrimInt>(a: T, b: T) -> T {
    (a + b - T::one()) / b
}

#[inline(always)]
pub fn align_down(a: usize, alignment: usize) -> usize {
    a & !(alignment - 1)
}

#[inline(always)]
pub fn align_up(a: usize, alignment: usize) -> usize {
    (a + alignment - 1) & !(alignment - 1)
}

#[inline(always)]
pub fn ptr_to_usize<T>(r: &T) -> usize {
    (r as *const T).addr()
}

#[inline(always)]
pub fn ptr_to_ref_mut<T, A>(r: &T) -> *mut A {
    r as *const _ as *const A as *mut A
}

#[inline(always)]
pub fn usize_to_ref_mut<A>(r: usize) ->  &'static mut A {
    unsafe {&mut *(r as *mut A)}
}

#[macro_export]
macro_rules! en_flag {
    ($cond:expr, $($flags:expr),+) => {
        if $cond {
            $($flags |)* 0
        }   
        else {
            0
        }
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


