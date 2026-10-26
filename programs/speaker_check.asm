BITS 32
ORG 0x40000000
%macro check 3
    mov eax,56
    mov ebx,%1
    mov ecx,%2
    int 0x80
    cmp eax,%3
    jne failed
%endmacro
    check 36,70,-22
    check 20001,70,-22
    check 880,1001,-22
    check 0,0,0
    check 880,0,0
    xor ebx,ebx
    jmp exit
failed:
    mov ebx,99
exit:
    mov eax,1
    int 0x80
    ud2
