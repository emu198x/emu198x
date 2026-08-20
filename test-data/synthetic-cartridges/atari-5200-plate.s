; The Emu198x divider plate for the Atari 5200.
;
; The 5200 will not start without a cartridge — and until now it could not
; start *with* one either: the reset vector lives in the BIOS socket, and the
; synthetic BIOS sets a colour and spins. `atari-5200-bios-handover.rom` is
; the companion to this file; it does nothing but jump through the start
; address a cartridge publishes at $BFFE.
;
; Nothing pokes a framebuffer. This programs ANTIC and GTIA and lets the chips
; draw: a display list, a character set, screen memory, and three colour
; registers. A pass says the BIOS handed over, cartridge reads landed across
; $4000-$BFFF, ANTIC fetched a display list and a font over DMA, and GTIA
; resolved the playfield registers.
;
; ## Why ANTIC mode 4
;
; The plate needs three colours at once — paper, the filled cell, ink — and
; the two-colour text modes cannot hold that. Mode 4 reads the font two bits
; per pixel, giving four pixels per byte from COLBK, COLPF0, COLPF1 and
; COLPF2. So a cell is 4 pixels wide here where it is 8 on the Game Boy and
; NES, and the same 128-pixel plate becomes 32 characters rather than 16.
; ANTIC doubles them on the way out, so the plate covers more of this screen
; than of the others. That is the hardware's proportion, not a design choice.

; GTIA sits at $C000 on this machine rather than the 8-bit line's $D000.
COLPF0 = $C016
COLPF1 = $C017
COLBK  = $C01A

DMACTL = $D400
DLISTL = $D402
DLISTH = $D403
CHBASE = $D409

* = $4000
start:
    sei
    cld
    ldx #$FF
    txs

    ; DMA off while the display list and font are pointed at, so ANTIC never
    ; fetches a half-built screen.
    lda #0
    sta DMACTL

    lda #PAPER_COLOUR
    sta COLBK
    lda #FILL_COLOUR
    sta COLPF0
    lda #INK_COLOUR
    sta COLPF1

    ; Mode 4 takes a 1 KB font, so `charset` is aligned to a 1 KB boundary by
    ; the builder rather than by hope.
    lda #>charset
    sta CHBASE

    lda #<dlist
    sta DLISTL
    lda #>dlist
    sta DLISTH

    ; Normal playfield width, display-list DMA on.
    lda #$22
    sta DMACTL

forever:
    jmp forever
