use crate::devices::HPET;
use super::asm;
use kernel_intf::info;
use core::sync::atomic::{AtomicUsize, Ordering};

static BASE_FREQ: AtomicUsize = AtomicUsize::new(0); 

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
    BASE_FREQ.store((num_ticks_passed * 10) as usize, Ordering::SeqCst);
    
    info!("CPU Clock frequency measured as {}Hz", BASE_FREQ.load(Ordering::Relaxed));
}
