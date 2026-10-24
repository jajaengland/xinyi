
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

    ; Preserve the multiboot magic and information pointer for the kernel.
    ; The padding keeps the call site 16-byte aligned as before, and must be
    ; below the arguments so rust_main reads magic/info at [esp+4]/[esp+8].
    sub esp, 8
    push ebx
    push eax
    call rust_main

.hang:
    cli
    hlt
    jmp .hang

; Load the kernel GDT and reload every segment register.
; void gdt_flush(const GdtPointer *pointer)
global gdt_flush
gdt_flush:
    mov eax, [esp + 4]
    lgdt [eax]

    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax

    ; Reload CS with the 0x08 code selector. A far jump is required because
    ; CS cannot be written directly.
    jmp 0x08:.reload_cs

.reload_cs:
    ret

; Load the Task State Segment.
global tss_flush
tss_flush:
    mov ax, 0x28
    ltr ax
    ret

; Enter ring 3 at the given entry point.
; void enter_user_mode(u32 entry)
;
; Builds the IRET frame manually:
;   SS, ESP, EFLAGS, CS, EIP
global enter_user_mode
enter_user_mode:
    mov edx, [esp + 4]      ; entry point

    mov ax, 0x23            ; user data selector (DPL 3)
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    push dword 0x23         ; SS
    push dword 0x700000     ; ESP (identity-mapped user stack)
    pushfd
    or dword [esp], 0x200   ; set IF
    push dword 0x1B         ; CS (user code, DPL 3)
    push edx                ; EIP
    iret

section .bss
align 16
stack_bottom:
    resb 16384                ; 16 KiB stack
global stack_top
stack_top:
