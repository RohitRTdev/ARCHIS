.org 0
.code16

// Modifications to trampoline need to be done with care
// Any instruction with a memory operand must be manually patched
// So, one needs to add a _patch<n> label next to it and then patch it in code 
// See smp::patch_trampoline() function for details 
ap_trampoline_start:
    cli
    cld

    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw $0, %sp
    // Here, assembler assumes 32 bits is required and 0x67 prefix is emitted
_patch1:
    lgdt gdt_desc

    // Enter protected mode
    movl %cr0, %eax
    orl  $0x1, %eax        
    movl %eax, %cr0
    // Ensure that offset is 32 bits (Force assembler to emit 0x66 prefix)
_patch2:
    ljmpl $CODE32_SEL, $pmode_entry

.align 8
gdt:
    .quad 0x0000000000000000      // null
    .quad 0x00cf9a000000ffff      // 32-bit code 
    .quad 0x00cf92000000ffff      // 32-bit data 
    .quad 0x00af9a000000ffff      // 64-bit code 
    .quad 0x0020920000000000      // 64-bit data

gdt_end:

gdt_desc:
    .word gdt_end - gdt - 1
    .long gdt

.set CODE32_SEL, 0x08
.set DATA32_SEL, 0x10
.set CODE64_SEL, 0x18
.set DATA64_SEL, 0x20

.code32
pmode_entry:
    movw $DATA32_SEL, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    movl %cr4, %eax
    orl  $(1 << 5), %eax       
    movl %eax, %cr4

_patch3:
    movl pml4_phys, %eax
    movl %eax, %cr3

    movl $0xC0000080, %ecx     // IA32_EFER
    rdmsr
    orl  $(1 << 8), %eax       // EFER.LME
    wrmsr

    movl %cr0, %eax
    orl  $(1 << 31), %eax      // CR0.PG
    movl %eax, %cr0

_patch4:
    ljmp $CODE64_SEL, $lmode_entry

.code64
lmode_entry:
    movw $DATA64_SEL, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    movq ap_stack_top(%rip), %rsp
    movq ap_entry(%rip), %rax
    jmp  *%rax

// All the addresses in this file will be patched by BSP before execution
.align 8
pml4_phys:
    .quad 0

ap_entry:
    .quad 0

ap_stack_top:
    .quad 0
