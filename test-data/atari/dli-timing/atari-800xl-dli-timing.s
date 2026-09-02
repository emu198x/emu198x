; Display-list interrupt timing probe for the Atari 800XL.
;
; Shows when a CHBASE write made from a DLI reaches the screen. The screen
; is mode 2 text, every character code 0, drawn from one of two fonts:
; font A's glyph 0 is solid and font B's is empty, so which font a scan
; line was drawn with is visible in every character of it. A DLI on the
; second text line switches CHBASE from A to B, and a DLI three lines
; later switches it back, so the frame repeats.
;
; How the DLI makes the write is taken from zero page, so one cartridge
; covers the cases the test wants:
;
;   $80  DMACTL — $22 for a normal playfield, $21 for a narrow one
;   $81  WSYNC  — nonzero: STA WSYNC before the stores
;   $82  DELAY  — 0-7 four-cycle stores before the CHBASE store
;
; With WSYNC the CHBASE store spills past the end of the interrupt's line
; into the first cycles of the next; without WSYNC it lands as early in
; the interrupt's own line as an interrupt can make a write. Either way
; the hardware fetches the line's glyph data after the write (Altirra
; Hardware Reference Manual, "Character mode playfield DMA": names from
; cycle 18 at normal width and 26 at narrow, glyph data three cycles
; later), so the line is drawn with the new font.
;
; The cartridge also carries the OS's run vector and flags at $BFFA-$BFFF,
; so it starts under the real OS as well as without one. The OS adds its
; own dispatch to a DLI, which moves the no-WSYNC write later in the line;
; the test runs without an OS.

COLPF1 = $D017
COLPF2 = $D018
COLBK  = $D01A
DMACTL = $D400
DLISTL = $D402
DLISTH = $D403
CHBASE = $D409
WSYNC  = $D40A
NMIEN  = $D40E

VDSLST = $0200

CFG_DMACTL = $80
CFG_WSYNC  = $81
CFG_DELAY  = $82

; The font the next DLI writes, and the value that turns it into the
; other one.
NEXT_FONT = $83

; Where the DLI enters the padding chain.
ENTRY = $84

; A byte the delay stores can hit without effect.
PAD = $0500

FONT_A = $3000
FONT_B = $3400
SCREEN = $3800

* = $A000
start:
    sei
    cld
    ldx #$FF
    txs
    lda #0
    sta NMIEN
    sta DMACTL

    ; Font A: glyph 0 solid. Font B: glyph 0 empty. Only glyph 0 is used.
    ldx #7
    lda #$FF
fonts:
    sta FONT_A,x
    lda #0
    sta FONT_B,x
    lda #$FF
    dex
    bpl fonts

    ; Six lines of 40 characters, all glyph 0.
    lda #0
    ldx #0
screen:
    sta SCREEN,x
    inx
    bne screen

    ; Lit text is COLPF1's luminance on COLPF2's hue; the rest of the
    ; playfield is COLPF2.
    lda #$0E
    sta COLPF1
    lda #$94
    sta COLPF2
    lda #0
    sta COLBK

    lda #>FONT_A
    sta CHBASE
    lda #>FONT_B
    sta NEXT_FONT

    lda #<dlist
    sta DLISTL
    lda #>dlist
    sta DLISTH

    ; The DLI handler, both for a machine with no OS (the NMI vector is in
    ; RAM there) and under the OS (VDSLST).
    ldx #<dli_fast
    ldy #>dli_fast
    lda CFG_WSYNC
    beq install
    ldx #<dli_wsync
    ldy #>dli_wsync
install:
    stx $FFFA
    stx VDSLST
    sty $FFFB
    sty VDSLST+1

    lda CFG_DMACTL
    sta DMACTL
    lda #$80
    sta NMIEN

forever:
    jmp forever

; Runs on the last scan line of each text line that asks for it, when the
; DLI is to wait for WSYNC. Everything but the stores is done before the
; wait, so the CPU comes back at cycle 105 with the font in A and the
; padding chain's entry point ready: the jump lands at cycle 110 and the
; CHBASE store's write is four cycles later for every store before it.
dli_wsync:
    pha
    txa
    pha
    ldx CFG_DELAY
    lda delays_lo,x
    sta ENTRY
    lda delays_hi,x
    sta ENTRY+1
    lda NEXT_FONT
    sta WSYNC
    jmp (ENTRY)

; Entered at one of eight points, so the CHBASE store follows 0-7 padding
; stores of four cycles each.
delay7:
    sta PAD
delay6:
    sta PAD
delay5:
    sta PAD
delay4:
    sta PAD
delay3:
    sta PAD
delay2:
    sta PAD
delay1:
    sta PAD
delay0:
    sta CHBASE
    jsr swap
    pla
    tax
    pla
    rti

; The same without the wait: the write is made as early in the line as an
; interrupt can make it. The handler saves nothing, so the main loop must
; not depend on A; that is what keeps the write ahead of the glyph fetch.
dli_fast:
    lda NEXT_FONT
    sta CHBASE
    jsr swap
    rti

; Swap the fonts over for the next interrupt.
swap:
    eor #(>FONT_A) XOR (>FONT_B)
    sta NEXT_FONT
    rts

delays_lo:
    !byte <delay0, <delay1, <delay2, <delay3, <delay4, <delay5, <delay6, <delay7
delays_hi:
    !byte >delay0, >delay1, >delay2, >delay3, >delay4, >delay5, >delay6, >delay7

; The OS calls the init address before the run address; there is nothing
; to do there.
init:
    rts

; 24 blank scan lines, then six text lines. The DLI bit is on the second
; and the fifth, so CHBASE changes on the last scan line of each and
; changes back in time for the next frame.
dlist:
    !byte $70, $70, $70
    !byte $42
    !word SCREEN
    !byte $82, $02, $02, $82, $02
    !byte $41
    !word dlist

; The OS looks here when it finds a cartridge: run address, the zero that
; says a cartridge is present, the option byte ($04: start the cartridge,
; no disk boot), and the init address.
* = $BFFA
    !word start
    !byte $00, $04
    !word init
