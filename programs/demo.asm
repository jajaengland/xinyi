BITS 32
ORG 0x40000000
    mov eax, 4
    mov ebx, 1
    mov ecx, message
    mov edx, message_end - message
    int 0x80
    cmp eax, message_end - message
    jne failed
    xor ebx, ebx
    jmp finished
failed:
    mov ebx, 1
finished:
    mov eax, 1
    int 0x80
    ud2
message: db 'Hello from a Xinyi program!', 10
message_end:
