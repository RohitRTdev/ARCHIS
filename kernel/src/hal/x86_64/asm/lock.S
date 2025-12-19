#include "asm_macros.inc"

// Acquire semantics are guaranteed since xchg also acts as memory fence
// This test, test-and-set technique is recommended for intel processors
FUNC acquire_lock
    cmpq $0, (%rdi)
    je do_acquire_lock
    pause 
    jmp acquire_lock
do_acquire_lock:
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