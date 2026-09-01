/* SPDX-License-Identifier: CC0-1.0 */

/* OCS custom-register offsets used by the probe. */
.equ INTREQR,       0x01e
.equ COP1LCH,       0x080
.equ COP1LCL,       0x082
.equ COPJMP1,       0x088
.equ DIWSTRT,       0x08e
.equ DIWSTOP,       0x090
.equ DDFSTRT,       0x092
.equ DDFSTOP,       0x094
.equ DMACON,        0x096
.equ INTENA,        0x09a
.equ INTREQ,        0x09c
.equ BPL1PTH,       0x0e0
.equ BPL1PTL,       0x0e2
.equ BPLCON0,       0x100
.equ BPLCON1,       0x102
.equ BPLCON2,       0x104
.equ BPL1MOD,       0x108
.equ BPL2MOD,       0x10a
.equ COLOR00,       0x180
.equ COLOR01,       0x182

.equ CUSTOM_BASE,   0x00dff000
.equ INTF_VERTB,    0x0020
.equ CLEAR_ALL,     0x7fff

/* Chip-RAM record exported for capture harnesses. */
.equ READY_BASE,                    0x0002ff00
.equ READY_MAGIC,                   0x43435048  /* "CCPH" */
.equ READY_CASE_NUMBER,             0x04
.equ READY_SCHEMA_VERSION,          0x06
.equ READY_FIELD_COUNTER,           0x08
.equ READY_DIWSTRT,                 0x0c
.equ READY_DIWSTOP,                 0x0e
.equ READY_DDFSTRT,                 0x10
.equ READY_DDFSTOP,                 0x12
.equ READY_BPLCON0,                 0x14
.equ READY_BPLCON1,                 0x16
.equ READY_BPLCON2,                 0x18
.equ READY_BPL1MOD,                 0x1a
.equ READY_BPL2MOD,                 0x1c
.equ READY_GUARD_COLOR00,           0x1e
.equ READY_MARKER_COLOR01,          0x20
.equ READY_MOVE_COLOR00_0,          0x22
.equ READY_MOVE_COLOR00_1,          0x24
.equ READY_MOVE_COLOR00_2,          0x26
.equ READY_MOVE_COLOR00_3,          0x28
.equ READY_DMACON_ENABLE,           0x2a
.equ READY_SAMPLE_LINE_FIRST,       0x2c
.equ READY_SAMPLE_LINE_LAST_EXCL,   0x2e
.equ READY_SAMPLE_BEAM_LINE,        0x30
.equ READY_RESET_WAIT_HPOS_CCK,     0x32
.equ READY_MOVE_WAIT_HPOS_CCK,      0x34
.equ READY_MOVE_COUNT,              0x36
.equ READY_MARKER_WORD_INDEX,       0x38
.equ READY_MARKER_BIT_INDEX,        0x3a
.equ READY_BITPLANE_ROWS,           0x3c
.equ READY_BITPLANE_WORDS_PER_ROW,  0x3e
.equ READY_BPL1_POINTER,            0x40
.equ READY_COPPER_POINTER,          0x44
.equ READY_IDENTITY,                0x50
.equ READY_SCHEMA_V1,               1
