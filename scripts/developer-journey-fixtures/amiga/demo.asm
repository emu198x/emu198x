; =============================================================================
; PLAYING A SAMPLE - AMIGA (Paula)
; Three one-second tones on channel 0, then silence.
;   1. 16-byte sine cycle, period 424: 3546895 / (424 * 16) = 522.8 Hz
;   2. the same 16 bytes, period 212:  1045.7 Hz, an octave higher
;   3. a 32-byte sine cycle, period 212: 522.8 Hz again
; Target: A500 PAL, Kickstart 1.3, started from the boot disk. The program
; takes over the whole machine and never returns to AmigaDOS.
; =============================================================================

CUSTOM          equ $dff000
INTREQR         equ $01e        ; Interrupt requests (read)
INTENA          equ $09a        ; Interrupt enable (write)
INTREQ          equ $09c        ; Interrupt requests (write)
COLOR00         equ $180
INTF_VERTB      equ $0020       ; Set by the display at the start of each frame

TONE_FRAMES     equ 50          ; One second per tone on PAL

            section code,code_c

start:
            lea     CUSTOM,a5

            ; --- Take over: no OS interrupts, no other DMA ---
            move.w  #$7fff,INTENA(a5)   ; Disable all interrupts
            move.w  #$7fff,INTREQ(a5)   ; Clear pending requests
            move.w  #$7fff,DMACON(a5)   ; Disable all DMA, audio included
            move.w  #$0035,COLOR00(a5)  ; Blue screen: the program is running

            ; --- 1. 16 samples per cycle at period 424 ---
            lea     wave16,a0
            moveq   #(wave16_end-wave16)/2,d0
            move.w  #424,d1
            moveq   #64,d2
            bsr     play_sample
            bsr     wait_tone

            ; --- 2. Same waveform, half the period: an octave up ---
            lea     wave16,a0
            moveq   #(wave16_end-wave16)/2,d0
            move.w  #212,d1
            moveq   #64,d2
            bsr     play_sample
            bsr     wait_tone

            ; --- 3. Twice the samples per cycle at period 212: back down ---
            lea     wave32,a0
            moveq   #(wave32_end-wave32)/2,d0
            move.w  #212,d1
            moveq   #64,d2
            bsr     play_sample
            bsr     wait_tone

            move.w  #$0050,COLOR00(a5)  ; Green screen: finished
hold:
            bra.s   hold

; wait_tone - let the tone play for TONE_FRAMES frames, stop it, then wait
; one more frame so the channel is off for well over two sample periods.
wait_tone:
            moveq   #TONE_FRAMES-1,d7
.frame:     bsr.s   wait_frame
            dbra    d7,.frame
            bsr     stop_sample
            ; fall through for the gap frame

; wait_frame - poll the vertical-blank request. Interrupts are disabled,
; but Paula still records the request in INTREQR.
wait_frame:
            move.w  #INTF_VERTB,INTREQ(a5)
.poll:      move.w  INTREQR(a5),d6
            and.w   #INTF_VERTB,d6
            beq.s   .poll
            rts

            include "paula-sample.inc"

; --- 8-bit signed samples. Audio DMA reads chip RAM only, a word at a time ---
            section data,data_c
            even
wave16:     dc.b    0,49,90,117,127,117,90,49,0,-49,-90,-117,-127,-117,-90,-49
wave16_end:
            even
wave32:     dc.b    0,25,49,71,90,106,117,125,127,125,117,106,90,71,49,25
            dc.b    0,-25,-49,-71,-90,-106,-117,-125,-127,-125,-117,-106,-90,-71,-49,-25
wave32_end:
