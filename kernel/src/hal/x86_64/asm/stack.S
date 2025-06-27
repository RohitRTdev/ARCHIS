#include "asm_macros.inc"

FUNC switch_stack_and_jump
    movq %rdi, %rsp
    jmp *%rsi
ENDF switch_stack_and_jump