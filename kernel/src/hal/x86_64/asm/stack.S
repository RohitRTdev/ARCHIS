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
