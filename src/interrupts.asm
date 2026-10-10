BITS 32

EXTERN xinyi_exception_handler

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

    ; After pusha:
    ; [esp + 32] = exception number
    ; [esp + 36] = error code
    mov eax, [esp + 32]
    mov edx, [esp + 36]

    ; Rust C ABI arguments: (vector, error_code).
    push edx
    push eax
    call xinyi_exception_handler

    ; The Rust handler returns !; it should never return.
    cli

.halt:
    hlt
    jmp .halt