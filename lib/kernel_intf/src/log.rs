#![allow(static_mut_refs)]

#[macro_export]
macro_rules! print {
    () => {};
    ($($arg:tt)*) => {
        #[cfg(not(test))]
        {
            unsafe {
                use core::fmt::Write;

                $crate::acquire_spinlock(&mut $crate::LOGGER.lock);
                
                $crate::LOGGER.write_fmt(::core::format_args!($($arg)*))
                .unwrap();
                
                $crate::release_spinlock(&mut $crate::LOGGER.lock);
            }
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
            use core::fmt::Write;
            unsafe {
                $crate::acquire_spinlock(&mut $crate::LOGGER.lock);
                LOGGER.lock().write_fmt(::core::format_args!("\n")).unwrap();
                $crate::release_spinlock(&mut $crate::LOGGER.lock);
            }
        }
    };
    ($($arg:tt)*) => {
        #[cfg(test)]
        {
            ::std::println!($($arg)*);
        }
        #[cfg(not(test))]
        {
            unsafe {
                use core::fmt::Write;

                $crate::acquire_spinlock(&mut $crate::LOGGER.lock);

                $crate::LOGGER.write_fmt(::core::format_args!($($arg)*))
                .and_then(|_| $crate::LOGGER.write_str("\n"))
                .unwrap();
                
                $crate::release_spinlock(&mut $crate::LOGGER.lock);
            }
        }
    };
}

#[macro_export]
macro_rules! level_print {
    ($level: literal, $($arg:tt)*) => {
        let timestamp = unsafe {
            $crate::acquire_spinlock(&mut $crate::LOGGER.lock);
            $crate::LOGGER.log_timestamp
        };

        unsafe {
            $crate::release_spinlock(&mut $crate::LOGGER.lock);
        }
        
        if timestamp {
            $crate::println!("[{}]-[{}]-[{}]-[{}]: {}", $level, unsafe {$crate::read_rtc()}, unsafe {$crate::read_timestamp()}, crate::hal::get_core(), format_args!($($arg)*));
        } else {
            $crate::println!("[{}]: {}", $level, format_args!($($arg)*));
        }
    };
}


#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::level_print!("INFO", $($arg)*);
    };
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::level_print!("DEBUG", $($arg)*);
    };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {};
}

impl core::fmt::Write for KernelLogger {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe {
            crate::serial_print_ffi(s.as_ptr(), s.len());
        }
        
        Ok(())
    }
} 

pub struct KernelLogger {
    pub log_timestamp: bool,
    pub lock: crate::Lock
}

pub static mut LOGGER: KernelLogger = KernelLogger { log_timestamp: false, lock: crate::Lock { lock: 0, int_status: false } };

pub fn init_logger() {
    unsafe {
        crate::create_spinlock(&mut LOGGER.lock);
    }
}

pub fn enable_timestamp() {
    unsafe {
        crate::acquire_spinlock(&mut crate::LOGGER.lock);    
        crate::LOGGER.log_timestamp = true;
        crate::release_spinlock(&mut crate::LOGGER.lock);
    }
}
