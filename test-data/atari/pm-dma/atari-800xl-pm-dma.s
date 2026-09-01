; Player/missile DMA probe for the Atari 800XL.
;
; Programs the whole chain a player crosses on its way to the screen —
; PMBASE, DMACTL, GRACTL, VDELAY, a display list — and lets the chips do the
; rest: ANTIC fetches the bitmaps over DMA, the machine hands them to GTIA,
; GTIA gates and positions them. Nothing pokes a graphics register. A lit
; pixel means every link held.
;
; The three registers under test are taken from zero page rather than
; assembled in, so one cartridge covers every combination a test wants to
; ask about: the test writes them before the first frame runs.
;
;   $80  DMACTL  — playfield width, which DMA is on, one- or two-line P/M
;   $81  GRACTL  — whether the DMA reaches GTIA's graphics registers
;   $82  VDELAY  — which objects are held back a line
;
; Every object is written into both P/M layouts at once — a 1 KB two-line
; block at $2000 and a 2 KB one-line block at $2800 — at positions chosen so
; an object covers the same scan lines whichever layout ANTIC reads. PMBASE
; is pointed at whichever block DMACTL bit 4 selects. A test can therefore
; keep one expectation across both resolutions, and the address arithmetic
; ANTIC does for each is what the test checks.
;
; The playfield is 24 rows of ANTIC mode 4 whose screen memory and font are
; both zero, so it paints COLBK everywhere: anything on screen that is not
; the background colour is a player or a missile. Two objects sit outside
; the mode lines on purpose — player 2 in the blank lines at the top, player
; 3 below the display list's jump — because P/M DMA happens on every scan
; line of the display whether or not ANTIC is drawing a mode line there.
;
; The cartridge also carries the OS's run vector and flags at $BFFA-$BFFF,
; so it starts under the real OS as well as without one.

HPOSP0 = $D000
HPOSP1 = $D001
HPOSP2 = $D002
HPOSP3 = $D003
HPOSM0 = $D004
COLPM0 = $D012
COLPM1 = $D013
COLPM2 = $D014
COLPM3 = $D015
COLBK  = $D01A
VDELAY = $D01C
GRACTL = $D01D
DMACTL = $D400
DLISTL = $D402
DLISTH = $D403
PMBASE = $D407
CHBASE = $D409
NMIEN  = $D40E

CFG_DMACTL = $80
CFG_GRACTL = $81
CFG_VDELAY = $82

; The two blocks cannot share a base: the one-line missiles at +$300 would
; sit on the two-line players 2 and 3. ANTIC ignores PMBASE's low bits —
; two of them for the 1 KB two-line block, three for the 2 KB one-line
; block — so the values written carry junk there, and the picture is right
; only if ANTIC masks them.
PM_TWO = $2000
PM_ONE = $2800
PMBASE_TWO = $21
PMBASE_ONE = $2B
SCREEN = $3000

; Two-line offsets: missiles at +$180, players at +$200, +$280, +$300, +$380.
; One-line offsets: missiles at +$300, players at +$400, +$500, +$600, +$700.
M_TWO  = PM_TWO + $180
P0_TWO = PM_TWO + $200
P1_TWO = PM_TWO + $280
P2_TWO = PM_TWO + $300
P3_TWO = PM_TWO + $380
M_ONE  = PM_ONE + $300
P0_ONE = PM_ONE + $400
P1_ONE = PM_ONE + $500
P2_ONE = PM_ONE + $600
P3_ONE = PM_ONE + $700

; Where each object sits. A two-line index covers scan lines 2n and 2n+1;
; a one-line index covers scan line n. The display's first scan line is 8,
; and the framebuffer's first row is scan line 8.
;
;   player 0, player 1, missile 0: scan lines 80-95, inside the mode lines
;   player 2: scan lines 16-23, in the blank lines above the playfield
;   player 3: scan lines 232-239, below the display list's jump
;
; Eight-byte objects in the two-line block, sixteen in the one-line block.
P0_TWO_START = 40
P0_ONE_START = 80
P2_TWO_START = 8
P2_ONE_START = 16
P3_TWO_START = 116
P3_ONE_START = 232

* = $A000
start:
    sei
    cld
    ldx #$FF
    txs
    lda #0
    sta NMIEN
    sta DMACTL
    sta GRACTL

    ; Clear both P/M blocks ($2000-$2FFF) and the screen ($3000-$33FF).
    ldx #0
    txa
clear:
    sta PM_TWO,x
    sta PM_TWO+$100,x
    sta PM_TWO+$200,x
    sta PM_TWO+$300,x
    sta PM_ONE,x
    sta PM_ONE+$100,x
    sta PM_ONE+$200,x
    sta PM_ONE+$300,x
    sta PM_ONE+$400,x
    sta PM_ONE+$500,x
    sta PM_ONE+$600,x
    sta PM_ONE+$700,x
    sta SCREEN,x
    sta SCREEN+$100,x
    sta SCREEN+$200,x
    sta SCREEN+$300,x
    inx
    bne clear

    ; Player 0: a solid bar.
    lda #$FF
    ldx #7
-   sta P0_TWO+P0_TWO_START,x
    dex
    bpl -
    ldx #15
-   sta P0_ONE+P0_ONE_START,x
    dex
    bpl -

    ; Player 1: only its two edge pixels, so bit order shows on screen.
    lda #$81
    ldx #7
-   sta P1_TWO+P0_TWO_START,x
    dex
    bpl -
    ldx #15
-   sta P1_ONE+P0_ONE_START,x
    dex
    bpl -

    ; Missile 0: both of its bits. The other missiles' bits stay clear.
    lda #$03
    ldx #7
-   sta M_TWO+P0_TWO_START,x
    dex
    bpl -
    ldx #15
-   sta M_ONE+P0_ONE_START,x
    dex
    bpl -

    ; Player 2, in the blank lines at the top.
    lda #$FF
    ldx #3
-   sta P2_TWO+P2_TWO_START,x
    dex
    bpl -
    ldx #7
-   sta P2_ONE+P2_ONE_START,x
    dex
    bpl -

    ; Player 3, below the display list.
    ldx #3
-   sta P3_TWO+P3_TWO_START,x
    dex
    bpl -
    ldx #7
-   sta P3_ONE+P3_ONE_START,x
    dex
    bpl -

    ; Black background, a distinct colour per player. Missile 0 takes
    ; player 0's colour, as missiles do unless PRIOR gives them their own.
    lda #$00
    sta COLBK
    lda #$C8
    sta COLPM0
    lda #$38
    sta COLPM1
    lda #$78
    sta COLPM2
    lda #$1C
    sta COLPM3

    ; Horizontal positions, in colour clocks from the left of the line.
    lda #$80
    sta HPOSP0
    lda #$40
    sta HPOSP1
    lda #$60
    sta HPOSP2
    lda #$A0
    sta HPOSP3
    lda #$C0
    sta HPOSM0

    ; The block that matches the resolution DMACTL asks for.
    ldx #PMBASE_TWO
    lda CFG_DMACTL
    and #$10
    beq +
    ldx #PMBASE_ONE
+   stx PMBASE
    ; A font of zeros: every glyph on screen is blank.
    lda #>SCREEN
    sta CHBASE
    lda #<dlist
    sta DLISTL
    lda #>dlist
    sta DLISTH

    ; The registers under test, last, so the picture starts whole.
    lda CFG_VDELAY
    sta VDELAY
    lda CFG_GRACTL
    sta GRACTL
    lda CFG_DMACTL
    sta DMACTL

forever:
    jmp forever

; The OS calls the init address before the run address; there is nothing
; to do there.
init:
    rts

; 24 blank scan lines, then 24 rows of mode 4 (192 scan lines), then wait
; for vertical blank and start again.
dlist:
    !byte $70, $70, $70
    !byte $44
    !word SCREEN
    !fill 23, $04
    !byte $41
    !word dlist

; The OS looks here when it finds a cartridge: run address, the zero that
; says a cartridge is present, the option byte ($04: start the cartridge,
; no disk boot), and the init address.
* = $BFFA
    !word start
    !byte $00, $04
    !word init
