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
ENDF halt


FUNC int
    int $14
    ret
ENDF int