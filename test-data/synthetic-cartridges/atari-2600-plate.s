; The Emu198x divider plate for the Atari 2600.
;
; Every other machine here has a framebuffer or a display processor. The 2600
; has neither: the TIA holds a handful of registers and the program rewrites
; them as the beam crosses, so the picture exists only as timing. A pass here
; proves cycle behaviour, not merely that memory was fetched.
;
; ## Why the playfield and not the sprites
;
; The finer technique is six sprite copies giving 48 pixels at one-clock
; resolution — how a 2600 game draws a logo. It was tried first and abandoned
; on measurement, not taste: those copies are eight colour clocks apart, and
; an `lda abs,y` plus `sta` is seven CPU cycles, or twenty-one clocks. The
; writes cannot chase the beam from memory, so the data has to be in registers
; before the line starts, and there are three registers for six slices.
;
; The playfield's forty blocks are four clocks each — coarse, but the writes
; have tens of cycles of slack rather than none. So the mark is drawn at the
; resolution this machine can actually hold, which is the honest answer for a
; console whose whole character is that constraint.
;
; The left twenty blocks repeat in the right half unless the program rewrites
; PF0/PF1/PF2 mid-line. This does, which is what makes forty distinct blocks:
; the left set is written during horizontal blank, the right set once the beam
; has passed the left half.
;
; ## Outlined, not filled
;
; `198x/decisions/family-visual-identity.md` offers an **outlined** rendering
; and calls it the default. That is the one here: a playfield line carries a
; single colour, so a filled prefix cell would need a mid-line COLUPF write
; competing with the playfield writes for the same cycles. Outlined is the
; decision's own provision rather than a compromise dressed as one.

VSYNC   = $00
VBLANK  = $01
WSYNC   = $02
COLUPF  = $08
COLUBK  = $09
CTRLPF  = $0A
PF0     = $0D
PF1     = $0E
PF2     = $0F

* = $F000
start:
    sei
    cld
    ldx #$FF
    txs
    ; The machine powers on with neither RAM nor the TIA defined. A kernel
    ; that assumed otherwise would work here and not on hardware.
    lda #0
    ldx #$FF
clear:
    sta $00,x
    dex
    bne clear
    sta $00

    lda #PAPER_COLOUR
    sta COLUBK
    lda #INK_COLOUR
    sta COLUPF
    ; Playfield priority and reflection both off: the right half repeats the
    ; left, which is exactly what the mid-line rewrite overrides.
    lda #0
    sta CTRLPF

frame:
    lda #2
    sta VSYNC
    sta WSYNC
    sta WSYNC
    sta WSYNC
    lda #0
    sta VSYNC

    lda #2
    sta VBLANK
    ldx #37
vblank:
    sta WSYNC
    dex
    bne vblank
    lda #0
    sta VBLANK

    ldx #TOP_BLANK
top:
    sta WSYNC
    dex
    bne top

    ; One flat loop, one table entry a scanline. The obvious shape is nested —
    ; eight rows of four lines — but the row transition has to run its
    ; bookkeeping in what is left of a line after six playfield stores, and
    ; there is not enough: it reached cycle 78 of a 76-cycle line, so its WSYNC
    ; waited out a whole further line. Every row drew five scanlines and the
    ; picture stood eight lines too tall. Flattening the tables removes the
    ; transition rather than shortening it.
    ldy #0
scanline:
    sta WSYNC
    ; Left half, written during horizontal blank. The beam reaches PF0's first
    ; block at colour clock 68 — cycle 22 — and PF2's at clock 116, cycle 38.
    ; These three stores finish at cycles 7, 14 and 21.
    lda pf0l,y
    sta PF0
    lda pf1l,y
    sta PF1
    lda pf2l,y
    sta PF2
    ; Right half, rewritten mid-line. Each store has to land after its own
    ; blocks have been drawn on the left and before they are drawn on the
    ; right: PF0 between clocks 84 and 148, PF1 between 116 and 164, PF2
    ; between 148 and 196. The four nops put the three stores at cycles 36, 43
    ; and 50 — clocks 108, 129 and 150 — inside all three windows with a dozen
    ; cycles of slack on PF0 and one on PF2, which is the binding one.
    nop
    nop
    nop
    nop
    lda pf0r,y
    sta PF0
    lda pf1r,y
    sta PF1
    lda pf2r,y
    sta PF2

    iny
    cpy #MARK_LINES
    bne scanline

    ; Clearing the playfield here rather than on the line the loop exits on.
    ; Falling out of the loop happens at cycle 57, so stores made straight away
    ; land at colour clocks 186, 195 and 204 — mid-picture, and the mark's last
    ; scanline lost its right-hand blocks to them. Behind a WSYNC the same
    ; three stores sit in horizontal blank, and repeating them on every blank
    ; line costs nothing.
    ldx #BOTTOM_BLANK
bottom:
    sta WSYNC
    lda #0
    sta PF0
    sta PF1
    sta PF2
    dex
    bne bottom

    lda #2
    sta VBLANK
    ldx #30
overscan:
    sta WSYNC
    dex
    bne overscan

    jmp frame
