OUTPUT_FORMAT("elf64-x86-64")
ENTRY(kern_main)

SECTIONS {
    .text : {
        KEEP(*(.text*))
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    .bss : {
        *(.bss*)
        *(COMMON)
    }
}