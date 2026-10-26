OUTPUT_FORMAT(elf32-i386)
ENTRY(_start)
PHDRS { image PT_LOAD FLAGS(5); }
SECTIONS {
    . = 0x40000000;
    .text : { KEEP(*(.text.start)) *(.text .text.*) } :image
    .rodata : { *(.rodata .rodata.*) } :image
    .data : { *(.data .data.* .got .got.* .init_array* .fini_array* .tdata*) } :image
    .bss (NOLOAD) : { *(.bss .bss.* .sbss* .tbss*) *(COMMON) } :image
    _image_end = .;
    ASSERT(SIZEOF(.data) == 0 && SIZEOF(.bss) == 0,
        "Current Xinyi programs support read-only images and stack locals, not writable globals")
    ASSERT(. <= 0x40010000, "Program exceeds 64 KiB image limit")
    /DISCARD/ : { *(.eh_frame* .comment .note*) }
}
