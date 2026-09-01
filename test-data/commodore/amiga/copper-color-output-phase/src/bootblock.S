/* SPDX-License-Identifier: CC0-1.0 */

/*
 * Kickstart enters an Amiga boot block with the open boot-device I/O request
 * in A1 and ExecBase in A6. This loader reads the sector-aligned probe from
 * sector two into chip RAM and transfers control only after DoIO succeeds.
 *
 * PAYLOAD_SECTORS is supplied by tools/build.py.
 */

.equ PAYLOAD_ADDRESS,    0x00030000
.equ FIRST_PAYLOAD_BYTE, 1024
.equ SECTOR_BYTES,       512

.equ IO_COMMAND,         0x1c
.equ IO_LENGTH,          0x24
.equ IO_DATA,            0x28
.equ IO_OFFSET,          0x2c
.equ CMD_READ,           2
.equ LVO_DO_IO,          -456

.section .text
.global _start
.type _start, @function

_start:
    .ascii "DOS"
    .byte 0
    .long 0
    .long 880

    move.w  #CMD_READ, IO_COMMAND(%a1)
    move.l  #(PAYLOAD_SECTORS * SECTOR_BYTES), IO_LENGTH(%a1)
    move.l  #PAYLOAD_ADDRESS, IO_DATA(%a1)
    move.l  #FIRST_PAYLOAD_BYTE, IO_OFFSET(%a1)
    jsr     LVO_DO_IO(%a6)
    tst.b   %d0
    bne.s   .Lread_failed
    jmp     PAYLOAD_ADDRESS

.Lread_failed:
    moveq   #-1, %d0
    rts

.size _start, .-_start
