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
    int 0x13
    jc failure
    cmp bx, 0xaa55
    jne failure
    test cx, 1
    jz failure
    mov dl, [drive]
    mov si, packet
    mov ah, 0x42
    int 0x13
    jc failure
    mov dl, [drive]
    jmp 0:0x8000
failure:
    mov si, message
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
message db 'Xinyi: BIOS EDD/stage-2 read failed', 0
times 440-($-$$) db 0
times 70 db 0
dw 0xaa55
