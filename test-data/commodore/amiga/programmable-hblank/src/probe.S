/* SPDX-License-Identifier: CC0-1.0 */

/*
 * The build creates case.inc from one validated cases.json entry. It defines
 * CASE_NUMBER, the six register words, and EMIT_CASE_IDENTITY.
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

    /*
     * Kickstart enters boot blocks in user mode, so interrupt masking must
     * use the custom-chip gate rather than a privileged SR write.
     */
    move.w  #CLEAR_ALL, DMACON(%a6)
    move.w  #CLEAR_ALL, INTENA(%a6)
    move.w  #CLEAR_ALL, INTREQ(%a6)

    /*
     * Program data first and selectors last. The configuration is allowed to
     * settle for complete fields before any capture becomes admissible.
     */
    /*
     * Select AGA palette bank zero before COLOR00. The case BPLCON3 value is
     * written later so this setup write cannot become the steady-state gate.
     */
    move.w  #0, BPLCON3(%a6)
    move.w  #CASE_COLOR00, COLOR00(%a6)
    move.w  #CASE_HBSTRT, HBSTRT(%a6)
    move.w  #CASE_HBSTOP, HBSTOP(%a6)
    move.w  #CASE_BEAMCON0, BEAMCON0(%a6)
    move.w  #CASE_BPLCON3, BPLCON3(%a6)
    move.w  #CASE_BPLCON0, BPLCON0(%a6)

    /* Publish a fixed-width, big-endian record in chip RAM. */
    move.l  #READY_BASE, %a0
    move.l  #READY_MAGIC, (%a0)
    move.w  #CASE_NUMBER, READY_CASE_NUMBER(%a0)
    move.w  #READY_SCHEMA_V1, READY_SCHEMA_VERSION(%a0)
    clr.l   READY_FIELD_COUNTER(%a0)
    move.w  #CASE_BPLCON0, READY_BPLCON0(%a0)
    move.w  #CASE_BPLCON3, READY_BPLCON3(%a0)
    move.w  #CASE_BEAMCON0, READY_BEAMCON0(%a0)
    move.w  #CASE_HBSTRT, READY_HBSTRT(%a0)
    move.w  #CASE_HBSTOP, READY_HBSTOP(%a0)
    move.w  #CASE_COLOR00, READY_COLOR00(%a0)

    lea     case_identity(%pc), %a1
    lea     READY_IDENTITY(%a0), %a2
.Lcopy_identity:
    move.b  (%a1)+, (%a2)+
    bne.s   .Lcopy_identity

    /*
     * Interrupts remain disabled, but Agnus still raises INTF_VERTB. Clear the
     * request, wait for the next field boundary, and publish a monotonic count.
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

.size _start, .-_start
