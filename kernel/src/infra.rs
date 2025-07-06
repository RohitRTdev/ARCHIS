use core::panic::PanicInfo;
use core::ffi::CStr;
use common::elf::*;
use rustc_demangle::demangle;
use crate::logger;
use crate::println;
use crate::sync::Spinlock;
use crate::{hal, CUR_STACK_BASE};
use crate::module::*;

static GLOBAL_PANIC_LOCK: Spinlock<bool> = Spinlock::new(false);

fn common_panic_handler(mod_name: &str, info: &PanicInfo) -> ! {
    let _sync = GLOBAL_PANIC_LOCK.lock();
    logger::set_panic_mode();
    let stack_base = *CUR_STACK_BASE.lock();
    let mut unwind_list: [usize; 8] = [0; 8];

    println!("Kernel panic!!");
    println!("Message: {}", info.message());
    println!("Module: {}", mod_name);

    #[cfg(debug_assertions)]
    {
        println!("Callstack:");

        let actual_depth = hal::unwind_stack(8, stack_base, unwind_list.as_mut_slice());
        let start_depth = if mod_name == env!("CARGO_PKG_NAME") { 3 } else { 4 };
        for addr in start_depth..actual_depth {
            if unwind_list[addr] != 0 {
                let sym_info = symbol_trace(unwind_list[addr]);
                if let Some(sym) = sym_info {
                    println!("{:#X}({}!{}+{:#X})", unwind_list[addr], sym.0, demangle(sym.1), sym.2);
                }
                else {
                    println!("{:#X}(??)", unwind_list[addr]);
                }
            }
        }
    }

    hal::halt();
}

fn symbol_trace(addr: usize) -> Option<(&'static str, &'static str, usize)> {
    let loaded_modules = MODULE_LIST.lock();

    for module in loaded_modules.iter() {
        // First find which module this symbol is part of
        if (addr >= module.info.base) && (addr < module.info.base + module.info.size) {
            // Now iterate through symbols to find the correct one
            if let Some(sym) = &module.info.sym_tab {
                let strtab = module.info.sym_str.as_ref().unwrap();

                let entries = unsafe {
                    core::slice::from_raw_parts(sym.start as *const Elf64Sym, sym.size / sym.entry_size)
                };

                let stringizer = |str_idx: usize| {
                    let str_base = unsafe {
                        (strtab.base_address as *const u8).add(str_idx)
                    };

                    unsafe {
                        CStr::from_ptr(str_base as *const i8).to_str().unwrap()
                    }
                };
                
                let shift = addr - module.info.base;
                for entry in entries {
                    let e_type = entry.st_info & 0x0f;
                    if e_type != STT_OBJECT && e_type != STT_FUNC {
                        continue;
                    }
                    
                    let lower_bound = entry.st_value as usize;
                    let upper_bound = lower_bound + entry.st_size as usize;
                    
                    // We found the data object or function this symbol belong to
                    if shift >= lower_bound && shift < upper_bound {
                        let offset = shift - lower_bound;
                        return Some((module.name, stringizer(entry.st_name as usize), offset))
                    }
                }
            }
        }
    }

    None
}


#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    common_panic_handler(env!("CARGO_PKG_NAME"), info)
}


#[cfg(not(test))]
#[no_mangle]
extern "C" fn panic_router(mod_name: *const u8, info: &PanicInfo) -> ! {
    common_panic_handler(unsafe {CStr::from_ptr(mod_name as *const i8).to_str().unwrap()}, info)
}