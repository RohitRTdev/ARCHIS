#![no_std]

// Call this macro anywhere you need to add unit-testing

#[macro_export]
macro_rules! init_test_logger {
    ($mod_name:ident) => {
        #[cfg(test)]
        extern crate std;

        #[ctor::ctor]
        fn init_test_logging() {
            let _ = env_logger::Builder::from_env(
                env_logger::Env::default().default_filter_or("debug")
            )
            .is_test(true)
            .try_init();
        
            println!("Starting tests for module {}", stringify!($mod_name));
        }
    };
}