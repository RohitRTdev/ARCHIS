.macro FUNC name
    .p2align 4, 0xCC
    .global \name
    .type \name, @function
\name:
.endm

.macro ENDF name
    .size \name, . - \name
.endm