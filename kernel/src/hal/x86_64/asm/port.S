#include "asm_macros.inc"

FUNC read_port_u8
    movw %di, %dx
    inb %dx, %al
    ret
ENDF read_port_u8



FUNC write_port_u8
    movw %di, %dx
    movb %sil, %al
    outb %al, %dx
    ret
ENDF write_port_u8

