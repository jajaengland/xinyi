; Copied to reserved low memory. All addresses are physical/identity mapped.
BITS 32
ORG 0x7000
%define C 0x6000
entry:
    pushad
    pushfd
    mov [C+40],esp
    sgdt [C+56]
    sidt [C+64]
    mov eax,cr0
    mov [C+44],eax
    mov eax,cr3
    mov [C+48],eax
    mov eax,cr4
    mov [C+52],eax
    mov dword [C+72],kernel_return
    mov word [C+76],0x08
    cli
    lgdt [private_gdtr]
    jmp word 0x08:protected16
BITS 16
protected16:
    mov ax,0x10
    mov ds,ax
    mov es,ax
    mov ss,ax
    mov sp,0xf000
    mov eax,cr0
    and eax,0x7ffffffe
    mov cr0,eax
    jmp 0:real
real:
    xor ax,ax
    mov ss,ax
    mov sp,0xf000
    mov ds,ax
    mov es,ax
    lidt [real_idtr]
    mov ax,[cs:C+34]
    mov es,ax
    mov ax,[cs:C+32]
    mov ds,ax
    mov eax,[cs:C]
    mov ebx,[cs:C+4]
    mov ecx,[cs:C+8]
    mov edx,[cs:C+12]
    mov esi,[cs:C+16]
    mov edi,[cs:C+20]
    mov ebp,[cs:C+24]
    cld
    sti
    cmp word [cs:C+36],0x13
    je .disk
    stc
    int 0x10
    jmp .returned
.disk:
    stc
    int 0x13
.returned:
    pushfd
    cli
    push ds
    push es
    push eax
    xor ax,ax
    mov ds,ax
    mov [C+4],ebx
    mov [C+8],ecx
    mov [C+12],edx
    mov [C+16],esi
    mov [C+20],edi
    mov [C+24],ebp
    pop eax
    mov [C],eax
    pop ax
    mov [C+34],ax
    pop ax
    mov [C+32],ax
    pop eax
    mov [C+28],eax
    ; Mask NMI across the protected-mode transition. The kernel normally
    ; keeps NMI enabled and selects CMOS registers explicitly for each read.
    mov al,0x80
    out 0x70,al
    lgdt [private_gdtr]
    mov eax,cr0
    or eax,1
    mov cr0,eax
    jmp dword 0x18:protected32
BITS 32
protected32:
    mov ax,0x20
    mov ds,ax
    mov es,ax
    mov ss,ax
    mov esp,[C+40]
    lgdt [C+56]
    mov eax,[C+52]
    mov cr4,eax
    mov eax,[C+48]
    mov cr3,eax
    mov eax,[C+44]
    mov cr0,eax
    jmp far [C+72]
kernel_return:
    mov ax,0x10
    mov ds,ax
    mov es,ax
    mov fs,ax
    mov gs,ax
    mov ss,ax
    lidt [C+64]
    cld
    popfd
    popad
    ret
align 8
private_gdt:
    dq 0
    dq 0x00009a000000ffff
    dq 0x000092000000ffff
    dq 0x00cf9a000000ffff
    dq 0x00cf92000000ffff
private_gdtr:
    dw private_gdtr-private_gdt-1
    dd private_gdt
real_idtr:
    dw 0x3ff
    dd 0
times 2048-($-$$) db 0
