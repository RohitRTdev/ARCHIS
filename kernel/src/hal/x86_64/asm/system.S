#include "asm_macros.inc"

FUNC init_address_space
    movq %cr4, %rax
    // Clear the PGE bit to invalidate all pages irrespective of how they were mapped by firmware
    movq %rax, %rcx
    andq $~(1 << 7), %rax
    movq %rax, %cr4
    movq %rdi, %rax
    andq $~0xfff, %rax
    // Set PWT bit to enable base table as WT (Write through)
    orq $0x8, %rax
    movq %rax, %cr3
    // Restore old CR4
    movq %rcx, %cr4
    // At this point we're in new address space, load the new stack and jump to target address
    movq %rsi, %rsp
    movq %rsi, %rbp
    jmp *%rdx
ENDF init_address_space


// Initialize GDTR, all 6 segment registers, IDTR and TR 
FUNC setup_table
    
    lgdt (%rdi)
    movw $0, %ax
    movw %ax, %ds
    movw %ax, %es
    
    // For fs and gs in 64 bit mode, this won't necessarily stop them from being used
    // They have a base address field that will be used when it's referenced
    // To configure that, we need to write an MSR
    movw %ax, %fs
    movw %ax, %gs

    // For CS, we can't just load it like other registers,
    // We setup the stack layout just like during an interrupt, and attempt to do an interrupt return
    // The CPU will load the SS, RSP, RFLAGS, CS and RIP registers in that order from the stack
    leaq (%rsp), %rax
    pushq $0x10
    pushq %rax
    pushfq 
    pushq $0x08
    leaq next(%rip), %rax
    pushq %rax    
    iretq

next:
    movw $0x28, %ax
    ltr %ax
    lidt (%rsi)
    ret
ENDF setup_table