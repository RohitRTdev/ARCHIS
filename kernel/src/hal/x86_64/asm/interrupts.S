#include "asm_macros.inc"
FUNC cli
    pushfq
    popq %rax
    cli
    ret
ENDF cli


FUNC sti
    sti
    ret
ENDF sti

FUNC halt
    cli
    hlt
    jmp halt
ENDF halt

FUNC sleep
    sti
    hlt
    jmp sleep
ENDF sleep

FUNC yield_cpu
    hlt
    ret
ENDF yield_cpu

FUNC fire_yield_interrupt
    int $33 
    ret
ENDF fire_yield_interrupt

FUNC fire_debug_interrupt
    int $34 
    ret
ENDF fire_debug_interrupt

