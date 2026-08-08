/* SPDX-License-Identifier: CC0-1.0 */

/*
 * The build creates case.inc from the validated cases.json entry. It defines
 * the fixed register words, marker geometry, and EMIT_CASE_IDENTITY.
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

    /* Remove any sprite state left by the boot display while DMA is off. */
    lea     SPR0POS(%a6), %a1
    moveq   #7, %d0
    moveq   #0, %d1
.Lclear_sprites:
    move.w  %d1, 0(%a1)
    move.w  %d1, 2(%a1)
    move.w  %d1, 4(%a1)
    move.w  %d1, 6(%a1)
    adda.w  #8, %a1
    dbra    %d0, .Lclear_sprites

    bsr     .Lrestore_dma_pointers

    move.w  #CASE_DIWSTRT, DIWSTRT(%a6)
    move.w  #CASE_DIWSTOP, DIWSTOP(%a6)
    move.w  #CASE_DDFSTRT, DDFSTRT(%a6)
    move.w  #CASE_DDFSTOP, DDFSTOP(%a6)
    move.w  #CASE_BPL1MOD, BPL1MOD(%a6)
    move.w  #CASE_BPL2MOD, BPL2MOD(%a6)
    move.w  #CASE_BPLCON1, BPLCON1(%a6)
    move.w  #CASE_BPLCON2, BPLCON2(%a6)

    /*
     * Open enhanced-register access, initialise the low AGA palette nibbles,
     * select palette bank zero, then install the common RGB4 values. OCS
     * ignores the enhanced-register writes.
     */
    move.w  #0x0001, BPLCON0(%a6)
    move.w  #0x0200, BPLCON3(%a6)
    move.w  #0x0000, COLOR00(%a6)
    move.w  #0x0000, COLOR01(%a6)
    move.w  #0x0000, COLOR17(%a6)
    move.w  #CASE_BPLCON3, BPLCON3(%a6)
    move.w  #CASE_COLOR00, COLOR00(%a6)
    move.w  #CASE_COLOR01, COLOR01(%a6)
    move.w  #CASE_COLOR17, COLOR17(%a6)
    move.w  #CASE_BPLCON4, BPLCON4(%a6)
    move.w  #CASE_FMODE, FMODE(%a6)
    move.w  #CASE_BPLCON0, BPLCON0(%a6)

    move.w  #CASE_DMACON_ENABLE, DMACON(%a6)

    /* Publish a fixed-width, big-endian input record in chip RAM. */
    move.l  #READY_BASE, %a0
    move.l  #READY_MAGIC, (%a0)
    move.w  #CASE_NUMBER, READY_CASE_NUMBER(%a0)
    move.w  #READY_SCHEMA_V1, READY_SCHEMA_VERSION(%a0)
    moveq   #0, %d0
    move.l  %d0, READY_FIELD_COUNTER(%a0)
    move.w  #CASE_SPR0POS, READY_SPR0POS(%a0)
    move.w  #CASE_SPR0CTL, READY_SPR0CTL(%a0)
    move.w  #CASE_SPR0DATA, READY_SPR0DATA(%a0)
    move.w  #CASE_SPR0DATB, READY_SPR0DATB(%a0)
    move.w  #CASE_DIWSTRT, READY_DIWSTRT(%a0)
    move.w  #CASE_DIWSTOP, READY_DIWSTOP(%a0)
    move.w  #CASE_DDFSTRT, READY_DDFSTRT(%a0)
    move.w  #CASE_DDFSTOP, READY_DDFSTOP(%a0)
    move.w  #CASE_BPLCON0, READY_BPLCON0(%a0)
    move.w  #CASE_BPLCON1, READY_BPLCON1(%a0)
    move.w  #CASE_BPLCON2, READY_BPLCON2(%a0)
    move.w  #CASE_BPLCON3, READY_BPLCON3(%a0)
    move.w  #CASE_BPLCON4, READY_BPLCON4(%a0)
    move.w  #CASE_FMODE, READY_FMODE(%a0)
    move.w  #CASE_COLOR00, READY_COLOR00(%a0)
    move.w  #CASE_COLOR01, READY_COLOR01(%a0)
    move.w  #CASE_COLOR17, READY_COLOR17(%a0)
    move.w  #CASE_DMACON_ENABLE, READY_DMACON_ENABLE(%a0)
    move.w  #CASE_MARKER_WORD_INDEX, READY_MARKER_WORD_INDEX(%a0)
    move.w  #CASE_MARKER_BIT_INDEX, READY_MARKER_BIT_INDEX(%a0)
    move.l  #bitplane_data, READY_BPL1_POINTER(%a0)
    move.l  #sprite_data, READY_SPR0_POINTER(%a0)
    move.w  #CASE_SPRITE_DATA_LINES, READY_SPRITE_DATA_LINES(%a0)
    move.w  #CASE_SAMPLE_BEAM_LINE, READY_SAMPLE_BEAM_LINE(%a0)

    lea     case_identity(%pc), %a1
    lea     READY_IDENTITY(%a0), %a2
.Lcopy_identity:
    move.b  (%a1)+, (%a2)+
    bne.s   .Lcopy_identity

    /*
     * Agnus advances both DMA pointers. Restore them at each field boundary,
     * early enough for the next bitplane and sprite control fetches.
     */
.Lnext_field:
    move.w  #INTF_VERTB, INTREQ(%a6)
.Lwait_vertb:
    move.w  INTREQR(%a6), %d0
    andi.w  #INTF_VERTB, %d0
    beq.s   .Lwait_vertb
    bsr     .Lrestore_dma_pointers
    addq.l  #1, READY_FIELD_COUNTER(%a0)
    bra.s   .Lnext_field

.Lrestore_dma_pointers:
    lea     bitplane_data(%pc), %a1
    move.l  %a1, %d0
    swap    %d0
    move.w  %d0, BPL1PTH(%a6)
    swap    %d0
    move.w  %d0, BPL1PTL(%a6)

    lea     sprite_data(%pc), %a1
    move.l  %a1, %d0
    swap    %d0
    move.w  %d0, SPR0PTH(%a6)
    swap    %d0
    move.w  %d0, SPR0PTL(%a6)

    lea     empty_sprite(%pc), %a1
    move.l  %a1, %d0
    lea     SPR1PTH(%a6), %a2
    moveq   #6, %d1
.Lrestore_empty_sprite:
    swap    %d0
    move.w  %d0, 0(%a2)
    swap    %d0
    move.w  %d0, 2(%a2)
    adda.w  #4, %a2
    dbra    %d1, .Lrestore_empty_sprite
    rts

.align 2
case_identity:
    EMIT_CASE_IDENTITY

.align 2
sprite_data:
    .word CASE_SPR0POS
    .word CASE_SPR0CTL
    .rept CASE_SPRITE_DATA_LINES
        .word CASE_SPR0DATA
        .word CASE_SPR0DATB
    .endr
    .word 0x0000
    .word 0x0000

.align 2
empty_sprite:
    .word 0x0000
    .word 0x0000

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
