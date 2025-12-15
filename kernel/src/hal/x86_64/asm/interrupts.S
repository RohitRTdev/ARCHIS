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

FUNC int
    int $14
    ret
ENDF int