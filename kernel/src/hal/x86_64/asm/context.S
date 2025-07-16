#include "asm_macros.inc"
.extern global_interrupt_handler

FUNC interrupt_context_save
    // Use rax as our scratch register
    pushq %rax

    // Get callstack saved info address
    leaq 16(%rsp), %rax
    
    // Save SS, RSP, RFLAGS, CS, RIP in this order
    pushq 32(%rax)
    pushq 24(%rax)
    pushq 16(%rax)
    pushq 8(%rax)
    pushq (%rax)

    // Save remaining registers
    pushq %rbp
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rdi
    pushq %rsi
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    // Pass vector and context information to main handler
    movq -8(%rax), %rdi
    leaq (%rsp), %rsi
    movq %rsp, %rbp
    call global_interrupt_handler

    leaq (%rsp), %rdi
    jmp switch_context 
ENDF interrupt_context_save


FUNC switch_context
    // Set stack to position suitable for iret
    leaq 112(%rdi), %rsp 
    
    // Restore all registers
    movq (%rdi), %r15
    movq 8(%rdi), %r14
    movq 16(%rdi), %r13
    movq 24(%rdi), %r12
    movq 32(%rdi), %r11
    movq 40(%rdi), %r10
    movq 48(%rdi), %r9
    movq 56(%rdi), %r8
    movq 64(%rdi), %rsi
    movq 80(%rdi), %rdx 
    movq 88(%rdi), %rcx
    movq 96(%rdi), %rbx
    movq 104(%rdi), %rbp
    movq 152(%rdi), %rax
    movq 72(%rdi), %rdi
    
    iretq 
ENDF switch_context