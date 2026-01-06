pub unsafe fn cpuid(fn_number: u32, opt_fn_number: u32) -> [u32; 4] {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    
    core::arch::asm!(
        "xchg {tmp:e}, ebx",
        "cpuid",
        "xchg {tmp:e}, ebx", 
        tmp = out(reg) ebx,
        inout("eax") fn_number => eax, inout("ecx") opt_fn_number => ecx,
        out("edx") edx, 
        options(nostack)
    );

    [eax, ebx, ecx, edx]
}

pub fn rdtsc() -> u64 {
    let eax: u32;
    let edx: u32;
    
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") eax,
            out("edx") edx,
            options(nomem, nostack)
        );
    }

    (eax as u64) | ((edx as u64) << 32)
}

pub unsafe fn rdmsr(address: u32) -> u64 {
    let eax: u32;
    let edx: u32;

    core::arch::asm!(
        "rdmsr",
        in("ecx") address,
        out("eax") eax, out("edx") edx,
        options(nomem, nostack)
    );

    (eax as u64) | ((edx as u64) << 32)
}

pub unsafe fn wrmsr(address: u32, value: u64) {
    let eax = value as u32;
    let edx = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") address,
        in("eax") eax,
        in("edx") edx,
        options(nostack)
    );
}

pub fn read_cr0() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr0", 
            out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

pub unsafe fn write_cr0(val: u64) {
    core::arch::asm!(
        "mov cr0, {}",
        in(reg) val, 
        options(nostack)
    );
}

pub fn read_cr2() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr2", 
            out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

pub fn read_cr3() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr3",
            out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

pub unsafe fn write_cr3(val: u64) {
    core::arch::asm!(
        "mov cr3, {}",
        in(reg) val,
        options(nostack)
    );
}

pub fn read_cr4() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "mov {}, cr4",
            out(reg) value,
            options(nomem, nostack)
        );
    }
    value
}

pub unsafe fn write_cr4(val: u64) {
    core::arch::asm!(
        "mov cr4, {}",
        in(reg) val,
        options(nostack)
    );
}

pub fn read_rflags() -> u64 {
    let value: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            out(reg) value,
            options(nomem, preserves_flags)
        );
    }
    value
}

pub unsafe fn write_rflags(val: u64) {
    core::arch::asm!(
        "push {}",
        "popfq",
        in(reg) val
    );
}

pub unsafe fn invlpg(addr: u64) {
    core::arch::asm!(
        "invlpg [{}]",
        in(reg) addr,
        options(nostack)
    );
}