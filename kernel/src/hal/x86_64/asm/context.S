#include "asm_macros.inc"
.extern global_interrupt_handler

FUNC interrupt_context_save
    // Save all general-purpose registers
    pushq %rax
    pushq %rbx
    pushq %rcx
    pushq %rdx
    pushq %rsi
    pushq %rdi
    pushq %rbp
    pushq %r8
    pushq %r9
    pushq %r10
    pushq %r11
    pushq %r12
    pushq %r13
    pushq %r14
    pushq %r15

    // rdi = vector
    // rsi = pointer to interrupt frame (including registers)
    movq 120(%rsp), %rdi     // vector (15 regs * 8)
    leaq 0(%rsp), %rsi       // context pointer

    call global_interrupt_handler

    jmp switch_context
ENDF interrupt_context_save


FUNC switch_context
    // Restore registers in reverse order
    popq %r15
    popq %r14
    popq %r13
    popq %r12
    popq %r11
    popq %r10
    popq %r9
    popq %r8
    popq %rbp
    popq %rdi
    popq %rsi
    popq %rdx
    popq %rcx
    popq %rbx
    popq %rax

    addq $8, %rsp   // discard vector

    iretq
ENDF switch_context
