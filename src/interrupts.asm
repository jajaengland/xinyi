BITS 32

EXTERN xinyi_exception_handler
EXTERN user_mode_return
EXTERN xinyi_user_should_return

SECTION .text

; Exceptions where the CPU does not push an error code.
%macro ISR_NOERR 1
GLOBAL isr%1
isr%1:
    push dword 0
    push dword %1
    jmp isr_common
%endmacro

; Exceptions where the CPU pushes an error code.
%macro ISR_ERR 1
GLOBAL isr%1
isr%1:
    push dword %1
    jmp isr_common
%endmacro

; Classic 32-bit x86 exception handlers.
ISR_NOERR 0
ISR_NOERR 1
ISR_NOERR 2
ISR_NOERR 3
ISR_NOERR 4
ISR_NOERR 5
ISR_NOERR 6
ISR_NOERR 7
ISR_ERR   8
ISR_NOERR 9
ISR_ERR   10
ISR_ERR   11
ISR_ERR   12
ISR_ERR   13
ISR_ERR   14
ISR_NOERR 15
ISR_NOERR 16
ISR_ERR   17
ISR_NOERR 18
ISR_NOERR 19
ISR_NOERR 20
ISR_NOERR 21
ISR_NOERR 22
ISR_NOERR 23
ISR_NOERR 24
ISR_NOERR 25
ISR_NOERR 26
ISR_NOERR 27
ISR_NOERR 28
ISR_NOERR 29
ISR_NOERR 30
ISR_NOERR 31

isr_common:
    pusha
    cld
    mov ebp, esp
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax

    ; Interrupt frame after pusha:
    ; [esp + 32] = exception number
    ; [esp + 36] = error code
    ; [esp + 40] = EIP
    ; [esp + 44] = CS
    ; [esp + 48] = EFLAGS
    and esp, -16
    sub esp, 12
    push dword [ebp + 48]
    push dword [ebp + 44]
    push dword [ebp + 40]
    push dword [ebp + 36]
    push dword [ebp + 32]
    call xinyi_exception_handler
    test eax, eax
    jnz user_mode_return
    cli

.halt:
    hlt
    jmp .halt

; Hardware IRQs use IDT vectors 32–47.
; Unlike CPU exceptions, these stubs return after handling the IRQ.

EXTERN xinyi_irq_handler
EXTERN xinyi_scheduled_irq
EXTERN xinyi_scheduled_syscall

%macro IRQ 1
GLOBAL irq%1
irq%1:
    push dword 0
    push dword (32 + %1)
    jmp irq_common
%endmacro

IRQ 0
IRQ 1
IRQ 2
IRQ 3
IRQ 4
IRQ 5
IRQ 6
IRQ 7
IRQ 8
IRQ 9
IRQ 10
IRQ 11
IRQ 12
IRQ 13
IRQ 14
IRQ 15

irq_common:
    pusha
    cld
    mov ebx, esp
    push ds
    push es
    push fs
    push gs
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov esi, esp
    and esp, -16
    sub esp, 12
    push dword [ebx + 32]
    call xinyi_irq_handler
    and esp,-16
    sub esp,8
    push esi
    push ebx
    call xinyi_scheduled_irq
    test eax,eax
    jnz user_mode_return
    mov esp, esi
    pop gs
    pop fs
    pop es
    pop ds
    popa

    ; Discard the vector and dummy error code.
    add esp, 8

    iretd

; System call handler for int 0x80
EXTERN xinyi_syscall_handler

GLOBAL isr128
isr128:
    ; CPU pushes SS, ESP, EFLAGS, CS, EIP when coming from lower privilege
    push dword 0      ; dummy error code
    push dword 0x80   ; vector
    pusha
    cld

    ; Pass the saved register frame, never the compiler's live registers.
    mov ebx, esp
    push ds
    push es
    push fs
    push gs
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov esi, esp
    ; i386 C ABI: 16-byte alignment immediately before CALL.
    and esp, -16
    sub esp, 12
    push ebx
    call xinyi_syscall_handler

    ; EBX/ESI survive the C call. Only saved EAX receives the result.
    mov [ebx + 28], eax
    and esp,-16
    sub esp,8
    push esi
    push ebx
    call xinyi_scheduled_syscall
    test eax,eax
    jnz user_mode_return
    call xinyi_user_should_return
    test eax, eax
    jnz user_mode_return
    mov esp, esi
    pop gs
    pop fs
    pop es
    pop ds

    popa
    add esp, 8
    iretd

; Position-independent blob copied into read-only user pages. Relative calls
; and branches must stay inside user_probe_start..user_probe_end.
SECTION .rodata.userprobe
GLOBAL user_probe_start
GLOBAL user_probe_end
user_probe_start:
    push ebx
    call usermode_syscall_probe
    mov ebx, eax             ; success marker: fd=1, zero-byte write
    mov eax, 4
    xor ecx, ecx
    xor edx, edx
    int 0x80
    pop edx                 ; supervisor physical alias supplied by kernel
    test ebx, ebx
    jz .bad
    cmp esi, 1
    je .kernel_read
    cmp esi, 2
    je .kernel_write
    cmp esi, 3
    je .kernel_stack
    cmp esi, 4
    je .page_table
    cmp esi, 5
    je .code_write
    cmp esi, 6
    je .stack_guard
    cmp esi, 7
    je .vga
    cmp esi, 8
    je .kernel_exec
    cmp esi, 9
    je .image_alias
    cmp esi, 10
    je .exit
    cmp esi, 11
    je .bad
    cmp esi, 12
    je .null_segments
    cmp esi, 13
    je .exit_df
    cli
.bad:
    ud2
.kernel_read:
    mov eax, [0x100000]
    jmp .bad
.kernel_write:
    mov dword [0x100000], 0
    jmp .bad
.kernel_stack:
    mov eax, [edi]
    jmp .bad
.page_table:
    mov eax, [ebp]
    jmp .bad
.code_write:
    mov byte [0x40000000], 0
    jmp .bad
.stack_guard:
    mov byte [0x403fb000], 0
    jmp .bad
.vga:
    mov eax, [0xb8000]
    jmp .bad
.kernel_exec:
    mov eax, 0x100000
    jmp eax
.image_alias:
    mov eax, [edx]
    jmp .bad
.null_segments:
    xor eax, eax
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ecx, 5000000
.spin:
    loop .spin
    jmp .exit
.exit_df:
    std
.exit:
    mov eax, 1
    mov ebx, 42
    int 0x80
    jmp .bad

; Ring-3 regression probe used by the usermode command. It checks argument
; delivery (debug console), return values, other GPRs, stack, segments and
; restored DF/IF across repeated calls. No kernel helpers run in ring 3.
GLOBAL usermode_syscall_probe
usermode_syscall_probe:
    push ebp
    push ebx
    push esi
    push edi
    sub esp, 8
    mov [esp], esp
    mov dword [esp + 4], 3
    xor eax, eax
    mov fs, ax
    mov gs, ax
.again:
    mov ebx, 0x11223344
    mov ecx, 0x55667788
    mov edx, 0x99aabbcc
    mov esi, 0x13579bdf
    mov edi, 0x2468ace0
    mov ebp, 0x76543210
    mov eax, 20
    std
    int 0x80
    cmp eax, 1
    jne .failed
    pushfd
    pop eax
    and eax, 0x600
    cmp eax, 0x600             ; DF and IF restored by IRET
    jne .failed
    cld                       ; Rust/C require a clear direction flag
    call .check_registers
    mov eax, 4
    int 0x80
    cmp eax, -9               ; the sentinel EBX is an invalid descriptor
    jne .failed
    call .check_registers
    mov eax, 0x7fffffff
    int 0x80
    cmp eax, -1
    jne .failed
    call .check_registers
    cmp esp, [esp]
    jne .failed
    mov ax, ds
    cmp ax, 0x23
    jne .failed
    mov ax, es
    cmp ax, 0x23
    jne .failed
    mov ax, fs
    test ax, ax
    jne .failed
    mov ax, gs
    test ax, ax
    jne .failed
    dec dword [esp + 4]
    jnz .again
    mov eax, 1
    jmp .done
.failed:
    xor eax, eax
.done:
    cld
    mov dx, 0x23
    mov fs, dx
    mov gs, dx
    add esp, 8
    pop edi
    pop esi
    pop ebx
    pop ebp
    ret
.check_registers:
    cmp ebx, 0x11223344
    jne .bad_register
    cmp ecx, 0x55667788
    jne .bad_register
    cmp edx, 0x99aabbcc
    jne .bad_register
    cmp esi, 0x13579bdf
    jne .bad_register
    cmp edi, 0x2468ace0
    jne .bad_register
    cmp ebp, 0x76543210
    jne .bad_register
    ret
.bad_register:
    add esp, 4                ; discard local helper return address
    jmp .failed
user_probe_end:

SECTION .bss
GLOBAL user_return_esp
GLOBAL user_return_cr3
align 4
user_return_esp: resd 1
user_return_cr3: resd 1
