; BIOS/EDD stage 1. The installer preserves bytes 440..509 of the target MBR.
BITS 16
ORG 0x7c00
    jmp 0:start
start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld
    sti
    mov [drive], dl
    mov bx, 0x55aa
    mov ah, 0x41
    push ds
    push es
    stc
    int 0x13
    pop es
    pop ds
    jc no_edd
    cmp bx, 0xaa55
    jne no_edd
    test cx, 1
    jz no_edd
    mov cx, 32
    call read_packet
    jnc loaded

    ; Some firmware advertises EDD but rejects multi-sector USB reads.
    ; Start over at LBA 1 so a partial bulk transfer cannot affect the result.
    mov cx, 1
.single:
    call read_packet
    jc read_failed
    add word [packet+4], 512
    inc word [packet+8]
    cmp word [packet+8], 33
    jb .single
loaded:
    mov dl, [drive]
    jmp 0:0x8000

; CX is the requested sector count. BIOS may rewrite the packet count.
read_packet:
    mov bp, 3
.retry:
    mov [packet+2], cx
    mov si, packet
    mov dl, [drive]
    mov ah, 0x42
    call disk_interrupt
    jnc .done
    dec bp
    jz .failed
    xor ax, ax
    mov dl, [drive]
    call disk_interrupt
    jmp .retry
.failed:
    stc
.done:
    ret

disk_interrupt:
    pusha
    push ds
    push es
    stc
    int 0x13
    pop es
    pop ds
    popa
    ret

no_edd:
    mov si, edd_message
    jmp failure
read_failed:
    mov si, read_message
failure:
    cld
.next:
    lodsb
    test al, al
    jz .halt
    out 0xe9, al
    mov ah, 0x0e
    xor bx, bx
    int 0x10
    jmp .next
.halt:
    cli
    hlt
    jmp .halt
align 4
packet:
    db 16, 0
    dw 32
    dw 0x8000, 0
    dq 1
drive db 0
edd_message db 'Xinyi: BIOS lacks extended disk reads (EDD)', 0
read_message db 'Xinyi: BIOS could not read stage 2', 0
times 440-($-$$) db 0
times 70 db 0
dw 0xaa55
