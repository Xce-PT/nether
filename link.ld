ENTRY(boot)

MEMORY {
    /* This value is purposefully kept small in order to warn me to change boot.s if the static regions grow too large. */
    ram : ORIGIN = 0x80000, LENGTH = 0x180000
}

SECTIONS {
    .text.boot 0x80000 : {*(.text.boot)} > ram = 0
    .text ALIGN(0x1000) : {*(.text .text.*)} > ram = 0
    .rodata ALIGN(0x1000) : {*(.rodata .rodata.*)} > ram = 0
    .data ALIGN(0x1000) : {*(.data .data.*)} > ram = 0
    .bss ALIGN(0x1000) : {*(.bss .bss.*)} > ram = 0
}

dma_start = 0x1000;
dma_end = 0x80000;
boot_start = ADDR(.text.boot);
boot_end = boot_start + SIZEOF(.text.boot) + 0xfff & ~0xfff;
text_start = ADDR(.text);
text_end = text_start + SIZEOF(.text) + 0xfff & ~0xfff;
rodata_start = ADDR(.rodata);
rodata_end = rodata_start + SIZEOF(.rodata) + 0xfff & ~0xfff;
data_start = ADDR(.data);
data_end = data_start + SIZEOF(.data) + 0xfff & ~0xfff;
bss_start = ADDR(.bss);
bss_end = bss_start + SIZEOF(.bss) + 0xfff & ~0xfff;
stack_x4 = 2M;
