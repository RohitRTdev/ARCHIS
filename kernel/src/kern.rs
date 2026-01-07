#![cfg_attr(not(test), no_std)]
#![feature(generic_const_exprs)]
#![feature(likely_unlikely)]
#![feature(allocator_api)]

mod infra;
mod hal;
mod sync;
mod mem;
mod ds;
mod module;
mod logger;
mod cpu;
mod devices;
mod sched;

#[cfg(feature = "acpi")]
mod acpica;

use core::sync::atomic::{AtomicBool, Ordering};

use kernel_intf::{info, debug};
use common::*;

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;


#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use cpu::install_interrupt_handler;
use crate::hal::{disable_interrupts, read_port_u8};
use crate::mem::Regions::*;
use crate::ds::*;
use crate::sched::KThread;
use crate::sync::KSem;

static BOOT_INFO: Once<BootInfo> = Once::new();

#[derive(Debug)]
enum RemapType {
    IdentityMapped,
    OffsetMapped(fn(usize))
}

#[derive(Debug)]
struct RemapEntry {
    value: MemoryRegion,
    map_type: RemapType,
    flags: u8
}

static INIT_FS: Once<BTreeMap<&'static str, &'static [u8]>> = Once::new();  
static REMAP_LIST: Spinlock<FixedList<RemapEntry, {Region3 as usize}>> = Spinlock::new(List::new());

fn clear_keyboard_output_buffer() {
    unsafe {
        while read_port_u8(0x64) & 0x01 != 0 {
            let _ = read_port_u8(0x60);
        }
    }
}

static TASK_COUNTER: Once<KSem> = Once::new();
static WAIT_EVENT: Once<KSem> = Once::new();

// Checking thread subsystem
fn task_spawn() -> ! {
    let mut tasks: VecDeque<KThread> = VecDeque::new();
    TASK_COUNTER.call_once(|| {
        KSem::new(0, 5)
    });

    WAIT_EVENT.call_once(|| {
        KSem::new(0, 1)
    });

    let task_id = sched::get_current_task_id().unwrap();
    info!("Starting task spawner, id={}", task_id);


    for idx in 0..5 {
        info!("Creating task {} in task spawner", idx);
        tasks.push_back(sched::create_thread(|| {
            let id = sched::get_current_task_id().unwrap(); 
            info!("Running task: {}", id);
            TASK_COUNTER.get().unwrap().signal();

            info!("id={}", id);
            
            loop {
                info!("id:{}", id);
            
                sched::delay_ms(1000);
            }
        }).unwrap());
    }

    info!("Task spawner going to wait!");
    
    for _ in 0..5 {
        TASK_COUNTER.get().unwrap().wait().unwrap();
    }

    info!("Task spawner starting kill spree");

    loop {
        info!("Task spawner waiting for keyboard event");
        KEYBOARD_EVENT.get().unwrap().wait().unwrap();
        info!("Task spawner springing to action");
        if !tasks.is_empty() {
            let task = tasks.pop_front().unwrap();
            let id = task.lock().get_id();
            info!("Killing task {}", id);
            sched::kill_thread(id);
        }
        else {
            info!("Killing self");
            sched::exit_thread();
            info!("This shouldn't be printed");
        }
    }
}

fn process_spawn() -> ! {
    for _ in 0..2 {
        sched::create_process(|| {
            let proc_id = sched::get_current_process_id().unwrap();
            let thread_id = sched::get_current_task_id().unwrap();
            info!("Created process with id {}", proc_id);  

            for _ in 0..2 {
                sched::create_thread(|| {
                    let thread_id = sched::get_current_task_id().unwrap();
                    let proc_id = sched::get_current_process_id().unwrap();
                    info!("Created new thread with id {}", thread_id);

                    loop {
                        info!("Running thread with id {} with process id {} on core {}", thread_id, proc_id, hal::get_core());
                        sched::delay_ms(1000);
                    }
                }).expect("Failed to create new thread");
            }

            loop {
                info!("Running thread with id {} with process id {}", thread_id, proc_id);
                info!("Process {} waiting for event..", proc_id);
                KEYBOARD_EVENT.get().unwrap().wait().unwrap();
                
                // Kill process 1 and then kill self
                sched::kill_process(1);
                sched::exit_process();
            }
        }).expect("Failed to create process");
    }

    // This pattern should be never followed in a real scenario, but this is just here for testing
    let sem = KSem::new(0, 1);

    info!("Init Thread going to wait state");
    let _ = sem.wait();

    sched::exit_thread();

    info!("Should never reach here");
    hal::halt();
}

fn kern_main() -> ! {
    info!("Starting main kernel init");
    
    KEYBOARD_EVENT.call_once(|| {
        KSem::new(0, 1)
    });

    // Sample invocation to test out interrupt subsystem
    clear_keyboard_output_buffer();
    install_interrupt_handler(1, key_notifier, true, true);

    sched::init();

    {
        sched::create_thread(watchdog).unwrap();
        let spawn_proc = sched::create_process(process_spawn).expect("Failed to create second process");
        info!("Main task waiting for process id 1 to complete");
        spawn_proc.wait().expect("Unable to wait on process id 1");
        
        let spawn_task = sched::create_thread(task_spawn).unwrap();

        info!("Main task waiting for task id 1 to complete");
        spawn_task.wait().expect("Unable to wait on task id 1");
    }

    info!("Main task going to sleep");
    hal::sleep();
}

// This will be called from the entry point for the corresponding arch
#[no_mangle]
unsafe extern "C" fn kern_start(boot_info: *const BootInfo) -> ! {
    disable_interrupts();
    BOOT_INFO.call_once(|| {
        *boot_info
    });   

    mem::setup_heap();
    logger::init();

    info!("Starting aris");
    devices::init();
    cpu::init();
    module::early_init();
    
    debug!("{:?}", *BOOT_INFO.get().unwrap());

    hal::init();
}

static KEYBOARD_EVENT: Once<KSem> = Once::new();

fn key_notifier(_: usize) {
    debug!("Got key notifier..");
    let avl_memory = mem::get_available_memory();
    info!("Available memory: {}", avl_memory);
    let task = sched::get_current_task();
    if task.is_none() {
        info!("Called keyboard handler from idle task on core {}", hal::get_core());
    }
    else {
        let task = task.unwrap();
        let id = task.lock().get_id();
        let status = task.lock().get_status();
        info!("Called keyboard handler in task:{} with status: {:?} on core {}", id, status, hal::get_core());
    }
    
    KEYBOARD_EVENT.get().unwrap().signal();
    clear_keyboard_output_buffer();

    // Let the watchdog task know that we're active
    WATCHDOG_MARK.store(true, Ordering::Release);
}

static WATCHDOG_MARK: AtomicBool = AtomicBool::new(false);

fn watchdog() -> ! {
    loop {
        sched::delay_ms(10_000);
        let is_active = WATCHDOG_MARK.load(Ordering::Acquire);
        
        if !is_active {
            info!("Watchdog task clearing keyboard buffer");
            clear_keyboard_output_buffer();
            WATCHDOG_MARK.store(true, Ordering::Release);
        }
        else {
            WATCHDOG_MARK.store(false, Ordering::Release);
        }
    }

}