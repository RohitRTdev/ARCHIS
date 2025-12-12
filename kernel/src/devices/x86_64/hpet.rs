use crate::BOOT_INFO;
use acpica::*;
use crate::Spinlock;
use kernel_intf::info;

const GEN_CAP_OFFSET: usize = 0;
const GEN_CONF_OFFSET: usize = 0x10;
const MAIN_CTR_OFFSET: usize = 0xF0;
const TIMER0_CONF_OFFSET: usize = 0x100;

pub struct Hpet {
    timer_block_base: usize,
    pub clk_period: usize, // femtoseconds
    num_timers: usize
}

impl Hpet {
    pub fn read_counter(&self) -> u64 {
        read_timer_reg(self.timer_block_base, MAIN_CTR_OFFSET)
    }
}

pub static HPET: Spinlock<Hpet> = Spinlock::new(Hpet { timer_block_base: 0, clk_period: 0, num_timers: 0});

fn read_timer_reg(base: usize, offset: usize) -> u64 {
    unsafe {
        *((base as *const u8).add(offset) as *const u64)
    }
}

fn write_timer_reg(base: usize, offset: usize, value: u64) {
    unsafe {
        *((base as *const u8).add(offset) as *mut u64) = value;
    }
}

#[cfg(feature = "acpi")]
pub fn init() {
    let hpet_tab = acpica::fetch_acpi_table::<AcpiTableHpet>(
        BOOT_INFO.get().unwrap().rsdp as *const u8).expect("No HPET ACPI table found!");

    
    assert_eq!(hpet_tab.address.space_id, AcpiAddressType::SYSTEM_MEMORY as u8, "HPET block address space not in system memory");

    let timer_block_base = hpet_tab.address.address as usize;

    let gen_cap = read_timer_reg(timer_block_base, GEN_CAP_OFFSET);        
    let clk_period = ((gen_cap >> 32) & 0xffffffff) as usize;
    let num_timers = ((gen_cap >> 8) & 0x1f) as usize;


    // Enable the timer block
    let gen_cnf = read_timer_reg(timer_block_base, GEN_CONF_OFFSET);
    write_timer_reg(timer_block_base, GEN_CONF_OFFSET, gen_cnf | 0x1);

    // Disable interrupts + set timer to 64 bit mode (if possible)
    let t0_cnf = read_timer_reg(timer_block_base, TIMER0_CONF_OFFSET);
    write_timer_reg(timer_block_base, TIMER0_CONF_OFFSET, (t0_cnf | (0x10)) & !(0x2u64));

    info!("HPET timer block found at address={:#X}, operating at time_period={}fs, and timer_count={}",
     timer_block_base, clk_period, num_timers);

    *HPET.lock() = Hpet {timer_block_base, clk_period, num_timers};
}