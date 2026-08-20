; The Emu198x divider plate for the Atari 7800.
;
; Nothing pokes a framebuffer. This builds MARIA's data structures in cartridge
; ROM and lets the chip fetch them: a display list list, one display list per
; scanline, graphics bytes, and three colour registers. A pass says the reset
; vector was fetched, cartridge reads landed across $4000-$FFFF, MARIA walked
; the DLL and the DLs over DMA, and a frame reached the screen.
;
; ## Why one zone per scanline
;
; MARIA renders in zones. Within a zone it adds the line's offset to the *high*
; byte of each graphics address, so consecutive scanlines of one zone sit 256
; bytes apart in memory. That layout suits a scrolling playfield and is pure
; overhead for a static picture: a 24-line plate would need six kilobytes of
; mostly padding.
;
; Giving every scanline its own single-line zone costs 24 display lists and
; lets the graphics sit contiguously, 32 bytes a row. The picture is fixed, so
; nothing is lost by not using the offset mechanism it exists to serve.
;
; 160A mode: two bits a pixel, four pixels a byte, three colours plus a
; transparent background — exactly the plate's paper, fill and ink.

BACKGRND = $20
P0C1     = $21
P0C2     = $22
DPPH     = $2C
DPPL     = $30
CTRL     = $3C

* = $4000
start:
    sei
    cld
    ldx #$FF
    txs

    ; DMA off while the pointers are set, so MARIA never walks a half-built
    ; display list.
    lda #$00
    sta CTRL

    lda #PAPER_COLOUR
    sta BACKGRND
    lda #FILL_COLOUR
    sta P0C1
    lda #INK_COLOUR
    sta P0C2

    lda #>dll
    sta DPPH
    lda #<dll
    sta DPPL

    lda #$40        ; DM = 10: MARIA renders
    sta CTRL

forever:
    jmp forever
