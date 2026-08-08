/* SPDX-License-Identifier: CC0-1.0 */

/* Custom-register offsets used by the probe. */
.equ INTREQR,       0x01e
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
.equ BPLCON3,       0x106
.equ BPL1MOD,       0x108
.equ BPL2MOD,       0x10a
.equ BPLCON4,       0x10c
.equ SPR0PTH,       0x120
.equ SPR0PTL,       0x122
.equ SPR1PTH,       0x124
.equ SPR0POS,       0x140
.equ COLOR00,       0x180
.equ COLOR01,       0x182
.equ COLOR17,       0x1a2
.equ FMODE,         0x1fc

.equ CUSTOM_BASE,   0x00dff000
.equ INTF_VERTB,    0x0020
.equ CLEAR_ALL,     0x7fff

/* Chip-RAM record exported for capture harnesses. */
.equ READY_BASE,                0x0002ff00
.equ READY_MAGIC,               0x53504858  /* "SPHX" */
.equ READY_CASE_NUMBER,         0x04
.equ READY_SCHEMA_VERSION,      0x06
.equ READY_FIELD_COUNTER,       0x08
.equ READY_SPR0POS,             0x0c
.equ READY_SPR0CTL,             0x0e
.equ READY_SPR0DATA,            0x10
.equ READY_SPR0DATB,            0x12
.equ READY_DIWSTRT,             0x14
.equ READY_DIWSTOP,             0x16
.equ READY_DDFSTRT,             0x18
.equ READY_DDFSTOP,             0x1a
.equ READY_BPLCON0,             0x1c
.equ READY_BPLCON1,             0x1e
.equ READY_BPLCON2,             0x20
.equ READY_BPLCON3,             0x22
.equ READY_BPLCON4,             0x24
.equ READY_FMODE,               0x26
.equ READY_COLOR00,             0x28
.equ READY_COLOR01,             0x2a
.equ READY_COLOR17,             0x2c
.equ READY_DMACON_ENABLE,       0x2e
.equ READY_MARKER_WORD_INDEX,   0x30
.equ READY_MARKER_BIT_INDEX,    0x32
.equ READY_BPL1_POINTER,        0x34
.equ READY_SPR0_POINTER,        0x38
.equ READY_SPRITE_DATA_LINES,   0x3c
.equ READY_SAMPLE_BEAM_LINE,    0x3e
.equ READY_IDENTITY,            0x40
.equ READY_SCHEMA_V1,           1
