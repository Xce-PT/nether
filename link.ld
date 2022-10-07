ENTRY(boot)

MEMORY {
    ram : ORIGIN = 0x0, LENGTH = 0x1160000
}

SECTIONS {
    .text.boot 0x80000 : {*(.text.boot)} > ram = 0
    .text ALIGN(0x1000) : {*(.text .text.*)} > ram = 0
    .rodata ALIGN(0x1000) : {*(.rodata .rodata.*)} > ram = 0
    .data ALIGN(0x1000) : {*(.data .data.*)} > ram = 0
    .bss ALIGN(0x1000) : {*(.bss .bss.*)} > ram = 0
}

bss_start = ADDR(.bss);
bss_end = bss_start + SIZEOF(.bss) + 0xfff & ~0xfff;
dynamic = 2M;
