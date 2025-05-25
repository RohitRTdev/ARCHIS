OUTPUT_FORMAT("elf64-x86-64")
ENTRY(kern_start)

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
    
    .dynamic : {
        *(.dynamic)
    } 
    
    .got : {
        *(.got.plt) 
        *(.got) 
    }
    
    .dynsym : {
        *(.dynsym) 
    }
    
    .dynstr : {
        *(.dynstr) 
    }
    
    .rela.dyn : {
        *(.rela.dyn) 
        *(.rela*) 
    }
    
    .plt : {
        *(.plt) 
    }
    
    
    /DISCARD/ : {
        *(.comment)
        *(.eh_frame)
        *(.eh_frame_hdr)
        *(.note*)
        *(.debug*)
        *(.gnu*)
    }
}