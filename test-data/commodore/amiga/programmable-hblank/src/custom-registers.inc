/* SPDX-License-Identifier: CC0-1.0 */

/* Custom register offsets used by the probe. */
.equ INTREQR,       0x01e
.equ DMACON,        0x096
.equ INTENA,        0x09a
.equ INTREQ,        0x09c
.equ BPLCON0,       0x100
.equ BPLCON3,       0x106
.equ COLOR00,       0x180
.equ HBSTRT,        0x1c4
.equ HBSTOP,        0x1c6
.equ BEAMCON0,      0x1dc

.equ CUSTOM_BASE,   0x00dff000
.equ INTF_VERTB,    0x0020
.equ CLEAR_ALL,     0x7fff

/* Chip-RAM record exported for capture harnesses. */
.equ READY_BASE,            0x0002ff00
.equ READY_MAGIC,           0x48424c4b  /* "HBLK" */
.equ READY_CASE_NUMBER,     0x04
.equ READY_SCHEMA_VERSION,  0x06
.equ READY_FIELD_COUNTER,   0x08
.equ READY_BPLCON0,         0x0c
.equ READY_BPLCON3,         0x0e
.equ READY_BEAMCON0,        0x10
.equ READY_HBSTRT,          0x12
.equ READY_HBSTOP,          0x14
.equ READY_COLOR00,         0x16
.equ READY_IDENTITY,        0x20
.equ READY_SCHEMA_V1,       1
