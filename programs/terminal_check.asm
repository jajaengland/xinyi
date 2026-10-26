; Foreground terminal ABI regression probe. Built and driven by terminal_qemu.py.
BITS 32
ORG 0x40000000
%macro call3 4
    mov eax, %1
    mov ebx, %2
    mov ecx, %3
    mov edx, %4
    int 0x80
%endmacro
%macro expect 1
    cmp eax, %1
    jne failed
%endmacro
start:
    call3 50, 0, 0, 0
    expect 0
    call3 50, 2, 0, 0
    expect -22
    call3 52, 60001, 0, 0
    expect -22
    call3 53, 3, 0, 0
    expect (80 | (25 << 16))
    call3 53, 1, 80, 0
    expect -22
    call3 54, 0x0a184f, letter, 2 ; Must not wrap a span past column 79.
    expect -22
    call3 54, 0x0a0000, 0x100000, 1
    expect -14
    call3 54, 0x0a0000, 0xffffffff, 0 ; Zero bytes ignore the pointer.
    expect 0
    call3 53, 0, 0x17, 0
    expect 0
    call3 53, 1, (3 | (2 << 16)), 0
    expect 0
    call3 54, 0x0a184f, letter, 1 ; Bottom right cell: no scroll/cursor change.
    expect 1
    call3 53, 4, 0, 0
    expect (3 | (2 << 16))
    call3 53, 5, 2, 0
    expect -22
    call3 53, 5, 0, 0
    expect 0
    call3 55, 0x010000, cells, 2
    expect -22
    call3 55, 0x184f, cells, 2
    expect -22
    call3 55, 0x0100, 0x100000, 1
    expect -14
    call3 55, 0x0100, bad_cells, 2
    expect -22
    call3 55, 0x0100, cells, 2
    expect 2
    call3 55, 0, 0xffffffff, 0
    expect 0
    call3 51, 0, 0, 0
    mov edi, eax
    std
    call3 52, 50, 0, 0
    expect 0
    pushfd
    pop eax
    and eax, 0x600
    expect 0x600 ; IF and DF survive a blocking syscall and nested PIT IRQs.
    cld
    call3 51, 0, 0, 0
    sub eax, edi
    cmp eax, 5
    jb failed
    call3 4, 1, ready, ready_end - ready
    mov esi, keys
.next:
    call3 50, 1, 0, 0
    cmp eax, [esi]
    jne failed
    add esi, 4
    cmp esi, keys_end
    jne .next
    call3 50, 0, 0, 0
    expect 0
    call3 4, 1, passed, passed_end - passed
    call3 1, 0, 0, 0
failed:
    cld
    call3 1, 99, 0, 0
    ud2
letter: db 'X'
cells: db 'C', 0x1e, 'D', 0x2f
bad_cells: db '!', 0x0f, '!', 0x80
ready: db 'Terminal checks ready', 10
ready_end:
passed: db 'Terminal checks passed!', 10
passed_end:
keys: dd (1 << 17) | 's', (1 << 16) | 'A', (1 << 18) | 'x'
      dd 0x104, 0x106, 0x107, 0x108, 0x109, 0x105, 0x120
keys_end:
