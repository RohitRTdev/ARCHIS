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

use kernel_intf::{info, debug};
use common::*;

extern crate alloc;
use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;


#[cfg(test)]
mod tests;

use sync::{Once, Spinlock};
use cpu::install_interrupt_handler;
use crate::hal::{delay_ns, disable_interrupts, read_port_u8};
use crate::mem::Regions::*;
use crate::ds::*;
use crate::sched::KThread;
use crate::sched::get_current_process;
use crate::sched::kill_process;
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
        KSem::new(-4, 1)
    });

    WAIT_EVENT.call_once(|| {
        KSem::new(0, 1)
    });

    info!("Active_tasks={}, Waiting_tasks={}, terminated_tasks={} in spawner", sched::get_num_active_tasks(), 
    sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks());

    let task_id = sched::get_current_task_id().unwrap();

    for idx in 0..5 {
        info!("Creating task {} in task spawner", idx);
        tasks.push_back(sched::create_thread(|| {
            let id = sched::get_current_task_id().unwrap(); 
            info!("Running task: {}", id);
            TASK_COUNTER.get().unwrap().signal();

            info!("id:{}, Active_tasks={}, Waiting_tasks={}, terminated_tasks={}", id, sched::get_num_active_tasks(), 
            sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks());
            
            loop {
                info!("id:{}, Active_tasks={}, Waiting_tasks={}, terminated_tasks={}, core={}", id, sched::get_num_active_tasks(), 
                sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks(), hal::get_core());
            
                sched::delay_ms(1000);
            }
        }).unwrap());
    }

    info!("Task spawner going to wait!");
    TASK_COUNTER.get().unwrap().wait().unwrap();
    info!("Active_tasks={}, Waiting_tasks={}, terminated_tasks={} in spawner", sched::get_num_active_tasks(), 
    sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks());

    loop {
        KEYBOARD_EVENT.get().unwrap().wait().unwrap();
        info!("id:{}, Active_tasks={}, Waiting_tasks={}, terminated_tasks={} before", task_id, sched::get_num_active_tasks(), 
        sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks());

        if !tasks.is_empty() {
            let task = tasks.pop_front().unwrap();
            let id = task.lock().get_id();
            info!("Killing task {}", id);
            sched::kill_thread(id);
        }
        else {
            info!("Killing thread 1");
            sched::kill_thread(1);
            info!("Killing self");
            sched::exit_thread();
            info!("This shouldn't be printed");
        }
        
        info!("id:{}, Active_tasks={}, Waiting_tasks={}, terminated_tasks={} after", task_id, sched::get_num_active_tasks(), 
        sched::get_num_waiting_tasks(), sched::get_num_terminated_tasks());
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
                info!("Running thread with id {} with process id {} on core {}", thread_id, proc_id, hal::get_core());
                
                info!("Waiting for event..");
                KEYBOARD_EVENT.get().unwrap().wait().unwrap();
                
                // Test self kill (exit)
                sched::exit_process();
            }
        }).expect("Failed to create process");
    }

    // This pattern should be never followed in a real scenario, but this is just here for testing
    let sem = KSem::new(0, 1);

    info!("Init Thread going to wait state");
    sem.wait().expect("Failed to wait on semaphore");

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
    //sched::create_thread(|| {
    //    loop {
    //        sched::delay_ms(1000); 
    //        info!("One second elapsed");
    //    }
    //}).unwrap();
    //sched::create_thread(|| {
    //    loop {
    //        sched::delay_ms(5000);
    //        info!("5 seconds elapsed");
    //    }
    //}).unwrap().wait().unwrap();

    {
        sched::create_process(process_spawn).expect("Failed to create second process");
        let spawn_task = sched::create_thread(task_spawn).unwrap();

        info!("Main task waiting for task id 1 to complete");
        spawn_task.wait().expect("Unable to wait on task id 1");
    }

    //sched::create_thread(|| {
    //    loop {
    //        info!("Running on core {}", hal::get_core());
    //        delay_ns(1_000_000_000);
    //    }
    //}).unwrap();

    //sched::create_thread(|| {
    //    let mut counter = 10;
    //    loop {
    //        info!("Running on core {}", hal::get_core());
    //        delay_ns(1_000_000_000);

    //        counter += 1;
    //        if counter >= 10 {
    //            let task = sched::get_current_task().unwrap().lock().get_id();
    //            sched::exit_thread();
    //        }
    //    }
    //}).unwrap();


    info!("Main task going to sleep");
    hal::sleep();
}

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
}