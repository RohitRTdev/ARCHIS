extern "C" {
    pub fn cli() -> u64;
    pub fn sti();

    pub fn read_port_u8(port: u16) -> u8;
    pub fn write_port_u8(port: u16, byte: u8);
    pub fn fire_yield_interrupt();
    pub fn fire_debug_interrupt();

    pub fn fetch_rbp() -> u64;
    pub fn fetch_rsp() -> u64;
    pub fn halt() -> !;
    pub fn sleep() -> !;
    pub fn yield_cpu();

    pub fn init_address_space(pml4_phys: u64, stack_address: u64, branch_addr: u64);
    pub fn setup_table(gdt_address: u64, idt_address: u64);

    pub fn cpuid(fn_number: u32, opt_fn_number: u32, result: *mut u8);
    pub fn write_cr0(val: u64);
    pub fn write_cr3(val: u64);
    pub fn write_cr4(val: u64);
    pub fn write_rflags(val: u64);
    pub fn read_rflags() -> u64;
    pub fn read_cr0() -> u64;
    pub fn read_cr2() -> u64;
    pub fn read_cr3() -> u64;
    pub fn read_cr4() -> u64;
    pub fn rdmsr(address: u32) -> u64;
    pub fn wrmsr(address: u32, data: u64);
    pub fn invlpg(address: u64);
    pub fn rdtsc() -> u64;
}
