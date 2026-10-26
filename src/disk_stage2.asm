; Xinyi BIOS loader. Layout matches bootdisk.rs; no filesystem dependency.
BITS 16
ORG 0x8000
%define INFO 0x4000
%define MAP 0x5000
%define HEADER 0x6000
%define LOAD 0x100000

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7c00
    cld
    mov [drive], dl
    sti
    mov ax, 3
    int 0x10
    ; Fast A20 gate: clear reset, enable the address line.
    in al, 0x92
    and al, 0xfe
    or al, 2
    out 0x92, al
    xor ax, ax
    mov es, ax
    mov di, INFO
    mov cx, 128/2
    rep stosw
    int 0x12
    movzx eax, ax
    mov [INFO+4], eax
    mov dword [INFO], 0x40
    mov dword [INFO+48], MAP
    xor ebx, ebx
    mov word [map_pointer], MAP
.map:
    mov di, [map_pointer]
    mov dword [di], 20
    add di, 4
    mov eax, 0xe820
    mov ecx, 20
    mov edx, 0x534d4150
    int 0x15
    jc memory_error
    cmp eax, 0x534d4150
    jne memory_error
    cmp ecx, 20
    jb memory_error
    xor ax, ax
    mov es, ax
    mov ds, ax
    add word [map_pointer], 24
    add dword [INFO+44], 24
    test ebx, ebx
    jz .map_done
    cmp word [map_pointer], MAP+128*24
    jb .map
    jmp memory_error
.map_done:
    mov word [packet+4], HEADER
    mov word [packet+6], 0
    mov dword [packet+8], 63
    call read_sector
    cmp dword [HEADER], 0x594e4958 ; XINY
    jne header_error
    cmp dword [HEADER+4], 0x00314b49 ; IK1, NUL
    jne header_error
    cmp dword [HEADER+8], 1
    jne header_error
    cmp dword [HEADER+12], LOAD
    jne header_error
    cmp dword [HEADER+32], 0x31544458 ; XDT1 selected-controller record
    jne header_error
    mov si, HEADER+32
    mov cx, 16
    mov eax, 0x811c9dc5
.target_hash:
    movzx edx, byte [si]
    xor eax, edx
    imul eax, eax, 0x01000193
    inc si
    loop .target_hash
    cmp eax, [HEADER+48]
    jne header_error
    cmp dword [HEADER+44], 3
    ja header_error
    mov eax, [HEADER+36]
    cmp eax, 0xffffffff
    je .legacy_target
    test eax, 0xff00e0f8
    jnz header_error
    cmp word [HEADER+40], 0
    je header_error
    cmp word [HEADER+40], 0xffff
    je header_error
    jmp .target_ok
.legacy_target:
    cmp dword [HEADER+40], 0
    jne header_error
.target_ok:
    mov si, HEADER+32
    mov di, INFO+128
    mov cx, 20
    cld
    rep movsb
    mov eax, [HEADER+20]
    test eax, eax
    jz header_error
    cmp eax, (2048-64)*512
    ja header_error
    add eax, LOAD
    cmp dword [HEADER+16], LOAD
    jb header_error
    cmp [HEADER+16], eax
    jae header_error
    mov eax, [HEADER+24]
    cmp eax, [HEADER+20]
    jb header_error
    cmp eax, 0x1000000
    ja header_error
    add eax, LOAD
    mov [memory_end], eax
    ; Require a single available E820 range covering the entire kernel,
    ; including BSS, boot stack, and its pristine-image snapshot.
    mov si, MAP
.ram:
    cmp dword [si+20], 1
    jne .next_ram
    cmp dword [si+8], 0
    jne .next_ram
    cmp dword [si+4], LOAD
    ja .next_ram
    mov eax, [si+4]
    xor edx, edx
    add eax, [si+12]
    adc edx, [si+16]
    test edx, edx
    jnz .large_ram
    cmp eax, [memory_end]
    jb .next_ram
    jmp .ram_ok
.large_ram:
    mov eax, 0xffffffff
.ram_ok:
    sub eax, LOAD
    shr eax, 10
    mov [INFO+8], eax
    or dword [INFO], 1
    jmp .load
.next_ram:
    add si, 24
    cmp si, [map_pointer]
    jb .ram
    jmp memory_error
.load:
    mov eax, [HEADER+20]
    add eax, 511
    shr eax, 9
    mov [remaining], ax
    mov dword [destination], LOAD
    mov word [packet+4], 0
    mov word [packet+6], 0x1000 ; Low-memory bounce sector at 0x10000.
    mov dword [packet+8], 64
.sector:
    call read_sector
    call unreal
    cld
    mov esi, 0x10000
    mov edi, [destination]
    mov ecx, 512
    a32 rep movsb
    xor ax, ax
    mov ds, ax
    mov es, ax
    add dword [destination], 512
    inc dword [packet+8]
    dec word [remaining]
    jnz .sector
    ; Detect a truncated or corrupt installed payload before jumping to it.
    call unreal
    mov esi, LOAD
    mov ecx, [HEADER+20]
    mov eax, 0x811c9dc5
.hash:
    movzx edx, byte [esi]
    xor eax, edx
    imul eax, eax, 0x01000193
    inc esi
    dec ecx
    jnz .hash
    cmp eax, [HEADER+28]
    jne checksum_error
    cli
    lgdt [gdt_pointer]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x18:protected

read_sector:
    sti
    mov word [packet+2], 1 ; BIOS may modify the transfer count.
    mov si, packet
    mov dl, [drive]
    mov ah, 0x42
    int 0x13
    jc disk_error
    xor ax, ax
    mov ds, ax
    mov es, ax
    ret

; Establish 4 GiB DS/ES limits for a bounded copy above 1 MiB, then return
; to real mode. Repeated after each BIOS call; BIOS need not preserve caches.
unreal:
    cli
    lgdt [gdt_pointer]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:.protected16
.protected16:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov eax, cr0
    and eax, 0xfffffffe
    mov cr0, eax
    jmp 0:.real
.real:
    ret

memory_error:
    mov si, memory_message
    jmp fatal
header_error:
    mov si, header_message
    jmp fatal
disk_error:
    mov si, disk_message
    jmp fatal
checksum_error:
    mov si, checksum_message
fatal:
    xor ax, ax
    mov ds, ax
    mov es, ax
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

align 8
gdt:
    dq 0
    dq 0x00009a000000ffff ; 16-bit code, base 0, 64 KiB.
    dq 0x00cf92000000ffff ; Flat 32-bit data.
    dq 0x00cf9a000000ffff ; Flat 32-bit code.
gdt_pointer:
    dw gdt_pointer-gdt-1
    dd gdt
align 4
packet:
    db 16,0
    dw 1
    dw 0,0
    dq 0
drive db 0
map_pointer dw 0
remaining dw 0
destination dd 0
memory_end dd 0
memory_message db 'Xinyi disk loader: missing/insufficient E820 RAM',0
header_message db 'Xinyi disk loader: invalid kernel header',0
disk_message db 'Xinyi disk loader: disk read failed',0
checksum_message db 'Xinyi disk loader: kernel checksum failed',0

BITS 32
protected:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x7c00
    cld
    mov edi, LOAD
    add edi, [HEADER+20]
    mov ecx, [HEADER+24]
    sub ecx, [HEADER+20]
    xor eax, eax
    rep stosb
    mov eax, 0x2badb002
    mov ebx, INFO
    jmp dword [HEADER+16]
times 32*512-($-$$) db 0
