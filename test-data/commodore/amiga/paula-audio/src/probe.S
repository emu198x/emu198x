/* SPDX-License-Identifier: CC0-1.0 */

/*
 * tools/build.py generates case.inc from one validated cases.json entry.
 * It defines the selected audio-register offsets and all case constants.
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
    move.l  #CIAA_BASE, %a5

    /*
     * Kickstart enters boot blocks in user mode. Mask custom-chip interrupts
     * at their source rather than attempting a privileged SR write.
     */
    move.w  #CLEAR_ALL, DMACON(%a6)
    move.w  #CLEAR_ALL, INTENA(%a6)
    move.w  #CLEAR_ALL, INTREQ(%a6)

    /* CIA-A port A bit 1 high disables the switchable LED filter. */
    ori.b   #CIAA_LED_BIT, CIAA_DDRA(%a5)
    ori.b   #CIAA_LED_BIT, CIAA_PRA(%a5)

    /* Stable visual identity; no bitplanes or other DMA remain active. */
    clr.w   BPLCON0(%a6)
    move.w  #CASE_COLOR00, COLOR00(%a6)

    lea     sample_data(%pc), %a1
    move.l  %a1, %d0
    swap    %d0
    move.w  %d0, CASE_AUD_LCH(%a6)
    swap    %d0
    move.w  %d0, CASE_AUD_LCL(%a6)
    move.w  #CASE_SAMPLE_WORDS, CASE_AUD_LEN(%a6)
    move.w  #CASE_PERIOD_CCK, CASE_AUD_PER(%a6)
    move.w  #CASE_VOLUME, CASE_AUD_VOL(%a6)

    /* Publish the complete fixed-width, big-endian ready record. */
    move.l  #READY_BASE, %a0
    move.l  #READY_MAGIC, (%a0)
    move.w  #CASE_NUMBER, READY_CASE_NUMBER(%a0)
    move.w  #READY_SCHEMA_V1, READY_SCHEMA_VERSION(%a0)
    clr.l   READY_FIELD_COUNTER(%a0)
    move.w  #CASE_CHANNEL, READY_CHANNEL(%a0)
    move.w  #CASE_PERIOD_CCK, READY_PERIOD_CCK(%a0)
    move.w  #CASE_VOLUME, READY_VOLUME(%a0)
    move.w  #CASE_SAMPLE_WORD, READY_SAMPLE_WORD(%a0)
    move.w  #CASE_SAMPLE_WORDS, READY_SAMPLE_WORDS(%a0)
    move.w  #CASE_DMACON_WORD, READY_DMACON_WORD(%a0)
    move.l  %a1, READY_SAMPLE_ADDRESS(%a0)

    lea     case_identity(%pc), %a2
    lea     READY_IDENTITY(%a0), %a3
.Lcopy_identity:
    move.b  (%a2)+, (%a3)+
    bne.s   .Lcopy_identity

    move.w  #CASE_DMACON_WORD, DMACON(%a6)

    /*
     * Paula interrupts remain disabled, but Agnus still raises INTF_VERTB.
     * Clear the request, wait for the next field, and publish a monotonic
     * field count without altering the audio configuration.
     */
.Lnext_field:
    move.w  #INTF_VERTB, INTREQ(%a6)
.Lwait_vertb:
    move.w  INTREQR(%a6), %d0
    andi.w  #INTF_VERTB, %d0
    beq.s   .Lwait_vertb
    addq.l  #1, READY_FIELD_COUNTER(%a0)
    bra.s   .Lnext_field

.align 2
case_identity:
    EMIT_CASE_IDENTITY

.align 2
sample_data:
    .rept CASE_SAMPLE_WORDS
    .word CASE_SAMPLE_WORD
    .endr

.size _start, .-_start
