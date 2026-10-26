
OUTPUT_FORMAT(elf32-i386)
ENTRY(_start)

SECTIONS
{
    . = 1M;

    _kernel_start = .;

    .multiboot ALIGN(4) :
    {
        KEEP(*(.multiboot))
        . = ALIGN(4K);
    }

    .text ALIGN(4K) :
    {
        *(.text .text.*)
        . = ALIGN(4K);
    }

    .rodata ALIGN(4K) :
    {
        *(.rodata .rodata.*)
        . = ALIGN(4K);
    }

    .data ALIGN(4K) :
    {
        *(.data .data.*)
    }

    _kernel_file_end = .;
    _kernel_file_size = _kernel_file_end - _kernel_start;

    .bss ALIGN(4K) :
    {
        *(COMMON)
        *(.bss .bss.*)
    }

    .bootcopy ALIGN(4K) (NOLOAD) :
    {
        _kernel_snapshot_start = .;
        . += _kernel_file_size;
    }

    _kernel_end = .;
    ASSERT(_kernel_file_size <= 1015808, "Kernel exceeds reserved disk boot area")
}
