extern "C" {
    pub fn cli() -> u64;
    pub fn sti();
    pub fn acquire_lock(lock: *mut u64);
    pub fn try_acquire_lock(lock: *mut u64) -> u64;

    pub fn read_port_u8(port: u16) -> u8;
    pub fn write_port_u8(port: u16, byte: u8);
    
    pub fn fetch_rbp() -> u64;
    pub fn fetch_rsp() -> u64;
    pub fn halt() -> !;

    pub fn switch_stack_and_jump(stack_addr: u64, branch_addr: u64);
}
