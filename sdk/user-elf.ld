OUTPUT_FORMAT(elf32-i386)
ENTRY(_start)
PHDRS { text PT_LOAD FLAGS(5); rodata PT_LOAD FLAGS(4); data PT_LOAD FLAGS(6); }
SECTIONS {
 . = 0x40000000;
 .text : { KEEP(*(.text.start)) *(.text .text.*) } :text
 . = ALIGN(4096);
 .rodata : { *(.rodata .rodata.*) } :rodata
 . = ALIGN(4096);
 .data : { *(.data .data.* .got .got.*) } :data
 .bss (NOLOAD) : { *(.bss .bss.* .sbss*) *(COMMON) } :data
 ASSERT(. <= 0x40100000, "ELF code/data/BSS exceeds 1 MiB")
 .unsupported : { *(.init_array* .fini_array* .tdata* .tbss*) }
 ASSERT(SIZEOF(.unsupported)==0, "TLS and global constructors are unsupported")
 /DISCARD/ : { *(.eh_frame* .comment .note*) }
}
