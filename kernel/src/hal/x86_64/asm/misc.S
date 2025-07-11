#include "asm_macros.inc"

FUNC fetch_rbp
    movq %rbp, %rax
    ret
ENDF fetch_rbp

FUNC fetch_rsp
    movq %rsp, %rax
    ret
ENDF fetch_rsp