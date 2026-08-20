; The Emu198x divider plate for the NES.
;
; The NES will not start without a cartridge, so "does it boot at all" was a
; claim public CI could not make. This cartridge is ours from source, and it
; draws through the real video path — CPU, PPU registers, nametable, CHR —
; rather than poking a framebuffer.
;
; Unlike the Game Boy's four greys, the NES has a palette, so the prefix cell
; carries a colour rather than a tone. `198x/decisions/family-visual-identity.md`
; gives Emu198x `#0d4a7d`; the nearest the NES offers is `$02`, a dark blue
; that white lettering still reads on. That is a real approximation of the
; project colour rather than a tonal stand-in.
;
; NROM: 32 KB PRG, 8 KB CHR, no mapper. The plate needs no bank switching and
; a cartridge that used one would be testing the mapper rather than the boot.
;
; Tiles, map and geometry are appended by build-synthetic-cartridges.py, which
; draws the plate once for every machine.

PPUCTRL   = $2000
PPUMASK   = $2001
PPUSTATUS = $2002
PPUADDR   = $2006
PPUDATA   = $2007
PPUSCROLL = $2005

NAMETABLE = $2000

.segment "HEADER"
    .byte "NES", $1A
    .byte 2          ; 32 KB PRG
    .byte 1          ; 8 KB CHR
    .byte 0, 0       ; mapper 0, horizontal mirroring
    .byte 0, 0, 0, 0, 0, 0, 0, 0

.segment "CODE"
reset:
    sei
    cld
    ldx #$FF
    txs
    inx              ; x = 0
    stx PPUCTRL      ; no NMI, rendering off while VRAM is written
    stx PPUMASK

    ; The PPU is not ready for two frames after power-on. Writing VRAM before
    ; then is the classic way to get a cartridge that works in one emulator
    ; and not on hardware.
:   bit PPUSTATUS
    bpl :-
:   bit PPUSTATUS
    bpl :-

    ; Background palette: paper, the filled cell, ink.
    lda #$3F
    sta PPUADDR
    lda #$00
    sta PPUADDR
    ldx #0
:   lda palette,x
    sta PPUDATA
    inx
    cpx #4
    bne :-

    ; Clear the nametable to tile 0, which the builder reserves as paper.
    lda #>NAMETABLE
    sta PPUADDR
    lda #<NAMETABLE
    sta PPUADDR
    lda #0
    ldx #4
    ldy #0
:   sta PPUDATA
    iny
    bne :-
    dex
    bne :-

    ; The plate, a row at a time. Three explicit blocks rather than a loop:
    ; nametable rows are 32 apart while the plate's are 16, so the address
    ; has to be reset per row anyway.
    lda #>PLATE_BASE
    sta PPUADDR
    lda #<PLATE_BASE
    sta PPUADDR
    ldx #0
:   lda PlateRow0,x
    sta PPUDATA
    inx
    cpx #PLATE_W
    bne :-

    lda #>(PLATE_BASE + 32)
    sta PPUADDR
    lda #<(PLATE_BASE + 32)
    sta PPUADDR
    ldx #0
:   lda PlateRow1,x
    sta PPUDATA
    inx
    cpx #PLATE_W
    bne :-

    lda #>(PLATE_BASE + 64)
    sta PPUADDR
    lda #<(PLATE_BASE + 64)
    sta PPUADDR
    ldx #0
:   lda PlateRow2,x
    sta PPUDATA
    inx
    cpx #PLATE_W
    bne :-

    ; Writing PPUADDR leaves the scroll latch pointing wherever it finished,
    ; so the picture would start mid-plate without this.
    lda #0
    sta PPUADDR
    sta PPUADDR
    sta PPUSCROLL
    sta PPUSCROLL

    lda #%00001110   ; background on, including the leftmost column
    sta PPUMASK

forever:
    jmp forever

nmi:
irq:
    rti

.segment "VECTORS"
    .word nmi, reset, irq
