#include "asm_macros.inc"

// Acquire semantics are guaranteed since xchg also acts as memory fence
FUNC acquire_lock
    movq $1, %rax
    lock xchgq %rax, (%rdi)
    testq %rax, %rax
    jnz acquire_lock
    ret
ENDF acquire_lock


FUNC try_acquire_lock
    movq $1, %rax
    lock xchgq %rax, (%rdi)
    testq %rax, %rax
    setnz %al
    movzbq %al, %rax
    ret
ENDF try_acquire_lock