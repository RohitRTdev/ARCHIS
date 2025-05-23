#include "asm_macros.inc"

FUNC read_port_u8
    pushq %rdx // Save rdx register
    movw %di, %dx
    inb %dx, %al
    popq %rdx // Restore it
    ret
ENDF read_port_u8



FUNC write_port_u8
    pushq %rdx
    movw %di, %dx
    movb %sil, %al
    outb %al, %dx
    popq %rdx
    ret
ENDF write_port_u8

