use std::{fs::File, io::Read};

use crate::load_kernel;
use std::alloc::{alloc, Layout};

tests::init_test_logger!(blr);


// Install hooks for functionality not available during test
#[cfg(test)]
#[no_mangle]
pub unsafe fn loader_alloc(layout: Layout) -> *mut u8 {
    alloc(layout)
}



#[test]
fn test1() {
    let mut file = File::open(format!("../../target/{}/debug/libaris.so", std::env::consts::ARCH)).unwrap();

    let mut buffer = Vec::new();

    file.read_to_end(&mut buffer).unwrap();

    println!("Read {} bytes", buffer.len());

    load_kernel(buffer.as_ptr());
}