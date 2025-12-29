#include "asm_macros.inc"


FUNC cpuid
    pushq %rbx
    pushq %rdx
    movl %edi, %eax
    movl %esi, %ecx 
    cpuid
    movq (%rsp), %rsi
    movl %eax, (%rsi)
    movl %ebx, 4(%rsi)
    movl %ecx, 8(%rsi)
    movl %edx, 12(%rsi)
    popq %rdx
    popq %rbx
    ret 
ENDF cpuid

FUNC rdtsc
    rdtsc
    shlq $32, %rdx
    orq %rdx, %rax
    ret
ENDF rdtsc

FUNC rdmsr
    movl %edi, %ecx
    rdmsr
    shlq $32, %rdx
    movl %eax, %eax
    orq %rdx, %rax
    ret
ENDF rdmsr

FUNC read_rflags
    pushfq
    popq %rax
    ret
ENDF read_rflags

FUNC read_cr0
    movq %cr0, %rax
    ret
ENDF read_cr0

FUNC read_cr2
    movq %cr2, %rax
    ret
ENDF read_cr2

FUNC read_cr4
    movq %cr4, %rax
    ret
ENDF read_cr4

FUNC wrmsr
    movl %esi, %eax
    shrq $32, %rsi
    movl %esi, %edx
    movl %edi, %ecx
    wrmsr
    ret
ENDF wrmsr

FUNC write_rflags
    pushfq
    movq %rdi, (%rsp)
    popfq
    ret
ENDF write_rflags

FUNC write_cr0
    movq %rdi, %cr0
    ret
ENDF write_cr0

FUNC write_cr3
    movq %rdi, %cr3
    ret
ENDF write_cr3

FUNC write_cr4
    movq %rdi, %cr4
    ret
ENDF write_cr4

FUNC invlpg
    invlpg (%rdi)
    ret
ENDF invlpg
