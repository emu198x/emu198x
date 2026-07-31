/* SPDX-License-Identifier: CC0-1.0 */

.equ CUSTOM_BASE,   0x00dff000
.equ CIAA_BASE,     0x00bfe001

.equ INTREQR,       0x01e
.equ DMACON,        0x096
.equ INTENA,        0x09a
.equ INTREQ,        0x09c
.equ BPLCON0,       0x100
.equ COLOR00,       0x180

.equ CIAA_PRA,      0x000
.equ CIAA_DDRA,     0x200
.equ CIAA_LED_BIT,  0x02

.equ INTF_VERTB,    0x0020
.equ CLEAR_ALL,     0x7fff

/* Chip-RAM record exported for capture harnesses. */
.equ READY_BASE,            0x0002ff00
.equ READY_MAGIC,           0x50415544  /* "PAUD" */
.equ READY_CASE_NUMBER,     0x04
.equ READY_SCHEMA_VERSION,  0x06
.equ READY_FIELD_COUNTER,   0x08
.equ READY_CHANNEL,         0x0c
.equ READY_PERIOD_CCK,      0x0e
.equ READY_VOLUME,          0x10
.equ READY_SAMPLE_WORD,     0x12
.equ READY_SAMPLE_WORDS,    0x14
.equ READY_DMACON_WORD,     0x16
.equ READY_SAMPLE_ADDRESS,  0x18
.equ READY_IDENTITY,        0x20
.equ READY_SCHEMA_V1,       1
