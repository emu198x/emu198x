/* SPDX-License-Identifier: CC0-1.0 */

/*
 * The build creates case.inc from the validated cases.json entry. It defines
 * all fixed register words, line geometry, colour words, and case identity.
 */
.include "custom-registers.inc"
.include "case.inc"

.equ STACK_TOP, 0x0002f000

.section .text
.global _start
.type _start, @function

_start:
    move.l  #STACK_TOP, %sp
    move.l  #CUSTOM_BASE, %a6

    /* Kickstart enters in user mode, so use the custom-chip gates. */
    move.w  #CLEAR_ALL, DMACON(%a6)
    move.w  #CLEAR_ALL, INTENA(%a6)
    move.w  #CLEAR_ALL, INTREQ(%a6)

    bsr     .Lrestore_bitplane_pointer

    move.w  #CASE_DIWSTRT, DIWSTRT(%a6)
    move.w  #CASE_DIWSTOP, DIWSTOP(%a6)
    move.w  #CASE_DDFSTRT, DDFSTRT(%a6)
    move.w  #CASE_DDFSTOP, DDFSTOP(%a6)
    move.w  #CASE_BPL1MOD, BPL1MOD(%a6)
    move.w  #CASE_BPL2MOD, BPL2MOD(%a6)
    move.w  #CASE_BPLCON1, BPLCON1(%a6)
    move.w  #CASE_BPLCON2, BPLCON2(%a6)
    move.w  #CASE_GUARD_COLOR00, COLOR00(%a6)
    move.w  #CASE_MARKER_COLOR01, COLOR01(%a6)
    move.w  #CASE_BPLCON0, BPLCON0(%a6)

    /* Publish the complete fixed-width, big-endian input record. */
    move.l  #READY_BASE, %a0
    move.l  #READY_MAGIC, (%a0)
    move.w  #CASE_NUMBER, READY_CASE_NUMBER(%a0)
    move.w  #READY_SCHEMA_V1, READY_SCHEMA_VERSION(%a0)
    moveq   #0, %d0
    move.l  %d0, READY_FIELD_COUNTER(%a0)
    move.w  #CASE_DIWSTRT, READY_DIWSTRT(%a0)
    move.w  #CASE_DIWSTOP, READY_DIWSTOP(%a0)
    move.w  #CASE_DDFSTRT, READY_DDFSTRT(%a0)
    move.w  #CASE_DDFSTOP, READY_DDFSTOP(%a0)
    move.w  #CASE_BPLCON0, READY_BPLCON0(%a0)
    move.w  #CASE_BPLCON1, READY_BPLCON1(%a0)
    move.w  #CASE_BPLCON2, READY_BPLCON2(%a0)
    move.w  #CASE_BPL1MOD, READY_BPL1MOD(%a0)
    move.w  #CASE_BPL2MOD, READY_BPL2MOD(%a0)
    move.w  #CASE_GUARD_COLOR00, READY_GUARD_COLOR00(%a0)
    move.w  #CASE_MARKER_COLOR01, READY_MARKER_COLOR01(%a0)
    move.w  #CASE_MOVE_COLOR00_0, READY_MOVE_COLOR00_0(%a0)
    move.w  #CASE_MOVE_COLOR00_1, READY_MOVE_COLOR00_1(%a0)
    move.w  #CASE_MOVE_COLOR00_2, READY_MOVE_COLOR00_2(%a0)
    move.w  #CASE_MOVE_COLOR00_3, READY_MOVE_COLOR00_3(%a0)
    move.w  #CASE_DMACON_ENABLE, READY_DMACON_ENABLE(%a0)
    move.w  #CASE_SAMPLE_LINE_FIRST, READY_SAMPLE_LINE_FIRST(%a0)
    move.w  #CASE_SAMPLE_LINE_LAST_EXCLUSIVE, READY_SAMPLE_LINE_LAST_EXCL(%a0)
    move.w  #CASE_SAMPLE_BEAM_LINE, READY_SAMPLE_BEAM_LINE(%a0)
    move.w  #CASE_RESET_WAIT_HPOS_CCK, READY_RESET_WAIT_HPOS_CCK(%a0)
    move.w  #CASE_MOVE_WAIT_HPOS_CCK, READY_MOVE_WAIT_HPOS_CCK(%a0)
    move.w  #CASE_MOVE_COUNT, READY_MOVE_COUNT(%a0)
    move.w  #CASE_MARKER_WORD_INDEX, READY_MARKER_WORD_INDEX(%a0)
    move.w  #CASE_MARKER_BIT_INDEX, READY_MARKER_BIT_INDEX(%a0)
    move.w  #CASE_BITPLANE_ROWS, READY_BITPLANE_ROWS(%a0)
    move.w  #CASE_BITPLANE_WORDS_PER_ROW, READY_BITPLANE_WORDS_PER_ROW(%a0)
    move.l  #bitplane_data, READY_BPL1_POINTER(%a0)
    move.l  #copper_list, READY_COPPER_POINTER(%a0)

    lea     case_identity(%pc), %a1
    lea     READY_IDENTITY(%a0), %a2
.Lcopy_identity:
    move.b  (%a1)+, (%a2)+
    bne.s   .Lcopy_identity

    move.l  #copper_list, %d0
    move.w  %d0, COP1LCL(%a6)
    swap    %d0
    move.w  %d0, COP1LCH(%a6)
    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)
    move.w  #0, COPJMP1(%a6)

    /* Restore the consumed bitplane pointer before every active window. */
.Lnext_field:
    move.w  #INTF_VERTB, INTREQ(%a6)
.Lwait_vertb:
    move.w  INTREQR(%a6), %d0
    andi.w  #INTF_VERTB, %d0
    beq.s   .Lwait_vertb
    bsr     .Lrestore_bitplane_pointer
    addq.l  #1, READY_FIELD_COUNTER(%a0)
    bra.s   .Lnext_field

.Lrestore_bitplane_pointer:
    move.l  #bitplane_data, %d0
    swap    %d0
    move.w  %d0, BPL1PTH(%a6)
    swap    %d0
    move.w  %d0, BPL1PTL(%a6)
    rts

.align 2
case_identity:
    EMIT_CASE_IDENTITY

.align 2
copper_list:
    .set CASE_COPPER_LINE, CASE_SAMPLE_LINE_FIRST
    .rept CASE_SAMPLE_LINE_COUNT
        .word ((CASE_COPPER_LINE << 8) | CASE_RESET_WAIT_HPOS_CCK | 1), 0xfffe
        .word COLOR00, CASE_GUARD_COLOR00
        .word ((CASE_COPPER_LINE << 8) | CASE_MOVE_WAIT_HPOS_CCK | 1), 0xfffe
        .word COLOR00, CASE_MOVE_COLOR00_0
        .word COLOR00, CASE_MOVE_COLOR00_1
        .word COLOR00, CASE_MOVE_COLOR00_2
        .word COLOR00, CASE_MOVE_COLOR00_3
        .set CASE_COPPER_LINE, CASE_COPPER_LINE + 1
    .endr
    .word 0xffff, 0xfffe

.align 2
bitplane_data:
    .rept CASE_BITPLANE_ROWS
        .rept CASE_MARKER_WORD_INDEX
            .word 0x0000
        .endr
        .word (1 << CASE_MARKER_BIT_INDEX)
        .rept CASE_MARKER_TRAILING_WORDS
            .word 0x0000
        .endr
    .endr

.size _start, .-_start
