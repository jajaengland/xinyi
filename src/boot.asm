
; src/boot.asm
BITS 32

section .multiboot
align 4
    dd 0x1BADB002            ; Multiboot magic
    dd 0x00000003            ; Align modules + memory info
    dd -(0x1BADB002 + 0x00000003)

section .text
global _start
extern rust_main

_start:
    cli
    mov esp, stack_top
    and esp, 0xFFFFFFF0

    call rust_main

.hang:
    cli
    hlt
    jmp .hang

section .bss
align 16
stack_bottom:
    resb 16384                ; 16 KiB stack
stack_top:
