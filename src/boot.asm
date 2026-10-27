
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
extern _kernel_start
extern _kernel_file_size
extern _kernel_snapshot_start
extern user_return_esp
extern user_return_cr3

_start:
    cli
    cld
    ; Preserve the initialized kernel before Rust mutates any global state.
    ; The linker reserves this copy inside the allocator's kernel range.
    mov ebp, eax
    mov edx, ebx
    mov esi, _kernel_start
    mov edi, _kernel_snapshot_start
    mov ecx, _kernel_file_size
    rep movsb
    mov eax, ebp
    mov ebx, edx
    mov edi, stack_bottom
    mov ecx, 16
    mov eax, 0x58475354
    rep stosd
    mov eax, ebp
    mov ebx, edx
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
; void enter_user_mode(u32 entry, u32 user_stack, u32 probe_mode,
;                      u32 kernel_stack, u32 directory, u32 image)
;
; Builds the IRET frame manually:
;   SS, ESP, EFLAGS, CS, EIP
global enter_user_mode
enter_user_mode:
    ; Preserve the caller's C ABI state on the original shell stack.
    push ebp
    push ebx
    push esi
    push edi
    pushfd
    push ds
    push es
    push fs
    push gs
    mov [user_return_esp], esp
    mov eax, cr3
    mov [user_return_cr3], eax
    cli
    mov edx, [esp + 40]     ; entry point
    mov ecx, [esp + 44]     ; user stack
    mov esi, [esp + 48]     ; probe mode
    mov edi, [esp + 52]     ; supervisor stack probe
    mov ebp, [esp + 56]     ; private directory
    mov ebx, [esp + 60]     ; physical image alias
    mov eax, ebp
    mov cr3, eax

    mov ax, 0x23            ; user data selector (DPL 3)
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    push dword 0x23         ; SS
    push ecx               ; owned identity-mapped user stack
    push dword 0x202        ; IF set; IOPL/NT/VM/DF clear
    push dword 0x1B         ; CS (user code, DPL 3)
    push edx                ; EIP
    iret

; Enter an arbitrary saved user context, preserving the existing host-return ABI.
; void enter_scheduled_user(const Cpu *context, u32 directory)
global enter_scheduled_user
enter_scheduled_user:
    push ebp
    push ebx
    push esi
    push edi
    pushfd
    push ds
    push es
    push fs
    push gs
    cli
    mov [user_return_esp],esp
    mov eax,cr3
    mov [user_return_cr3],eax
    mov esi,[esp+40]
    mov eax,[esp+44]
    mov cr3,eax
    sub esp,76
    mov edi,esp
    mov ecx,19
    cld
    rep movsd
    pop gs
    pop fs
    pop es
    pop ds
    popa
    add esp,8
    iretd

; Called only after Rust exception/syscall handlers have returned normally.
; Discard the finished user interrupt stack, never unwinding Rust frames.
global user_mode_return
user_mode_return:
    cli
    cld
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov eax, [user_return_cr3]
    mov cr3, eax
    mov esp, [user_return_esp]
    pop gs
    pop fs
    pop es
    pop ds
    popfd
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret

section .bss
align 16
global stack_bottom
stack_bottom:
    resb 65536                ; 64 KiB supervisor/scheduler stack
global stack_top
stack_top:
