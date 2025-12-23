use crate::devices::HPET;
use crate::sched::QUANTUM;
use super::asm;
use super::get_core;
use kernel_intf::info;
use super::lapic;
use core::sync::atomic::{AtomicUsize, Ordering};

static BASE_FREQ: AtomicUsize = AtomicUsize::new(0); 
static APIC_FREQ: AtomicUsize = AtomicUsize::new(0); 
pub static BASE_COUNT: AtomicUsize = AtomicUsize::new(0);

// Smallest granularity timer
pub fn delay_ns(value: usize) {
    // This stops kernel from pre-empting, so only use for small infrequent delays
    // We use the platform HPET for this purpose
    let hpet = HPET.lock();
    
    // Convert the wait time required to femtoseconds
    let total_time= (value * 1_000_000) as u64;
    
    let mut current_time  = 0u64;
    let start_ticks = hpet.read_counter();

    while current_time < total_time {
        let cur_ticks = hpet.read_counter();

        current_time = cur_ticks.wrapping_sub(start_ticks) * hpet.clk_period as u64;
        core::hint::spin_loop();
    }
}


pub fn init() {
    // Now measure APIC timer
    lapic::init_timer();

    if get_core() == 0 {
        // Measure the CPU clock frequency
        let old = unsafe {
            asm::rdtsc()
        };

        //Let's wait for 100ms
        delay_ns(100_000_000);
        
        let new = unsafe {
            asm::rdtsc()
        };

        let num_ticks_passed = new.wrapping_sub(old);
        BASE_FREQ.store((num_ticks_passed * 10) as usize, Ordering::Relaxed);
        
        info!("CPU Base Clock frequency measured as {}Hz", BASE_FREQ.load(Ordering::Relaxed));

        let old = lapic::get_timer_value();

        //Let's wait for 100ms
        delay_ns(100_000_000);

        let new = lapic::get_timer_value();
        // This is a countdown timer
        let num_ticks_passed = old.wrapping_sub(new);
        APIC_FREQ.store((num_ticks_passed * 10) as usize, Ordering::Relaxed);
        let freq = APIC_FREQ.load(Ordering::Relaxed);   
        
        info!("CPU APIC Clock frequency measured as {}Hz", freq);

        // Currently we use a divide factor of 128
        let init_count = (freq / 128) / (1000 / QUANTUM);
        assert!(init_count <= 0xffffffff);
        info!("Init count calculated as {:#X}", init_count);

        BASE_COUNT.store(init_count, Ordering::Relaxed);
    } 
    
    lapic::setup_timer();
}
