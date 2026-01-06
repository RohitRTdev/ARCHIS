extern "C" {
    pub fn init_address_space(pml4_phys: u64, stack_address: u64, branch_addr: u64);
    pub fn setup_table(gdt_address: u64, idt_address: u64);
}
