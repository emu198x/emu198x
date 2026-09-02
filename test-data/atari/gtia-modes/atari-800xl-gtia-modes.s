; GTIA mode probe for the Atari 800XL.
;
; Puts a known nibble ramp on a mode F screen and lets PRIOR decide how GTIA
; shows it: PRIOR bits 6-7 select the 16-luminance (9), nine-colour (10) and
; 16-hue (11) modes, and in each one a screen byte's two nibbles become two
; pixels, each two colour clocks wide. The picture that comes out says how
; GTIA pairs ANTIC's output into nibbles, which colour register each nibble
; reaches, and where the pixels land.
;
; The two registers under test are taken from zero page rather than
; assembled in, so one cartridge covers every mode: the test writes them
; before the first frame runs.
;
;   $80  PRIOR  — $00 for plain mode F, $40, $80 or $C0 for GTIA modes 9-11
;   $81  COLBK  — the background, which modes 9 and 11 build their colours on
;
; Every screen byte holds two consecutive nibbles, so pixel p of a row shows
; nibble p mod 16: $01 $23 $45 $67 $89 $AB $CD $EF, five times across 40
; bytes. The colour registers are set to distinct values so mode 10's
; mapping of nibbles to registers shows in the output.
;
; The cartridge also carries the OS's run vector and flags at $BFFA-$BFFF,
; so it starts under the real OS as well as without one.

COLPM0 = $D012
COLPM1 = $D013
COLPM2 = $D014
COLPM3 = $D015
COLPF0 = $D016
COLPF1 = $D017
COLPF2 = $D018
COLPF3 = $D019
COLBK  = $D01A
PRIOR  = $D01B
GRACTL = $D01D
DMACTL = $D400
DLISTL = $D402
DLISTH = $D403
NMIEN  = $D40E

CFG_PRIOR = $80
CFG_COLBK = $81

; 16 rows of mode F, 40 bytes each: 640 bytes from $3000.
SCREEN = $3000

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

    ; Fill three pages from SCREEN with the ramp; the screen is the first
    ; 640 bytes of them.
    ldx #0
fill:
    txa
    and #7
    tay
    lda ramp,y
    sta SCREEN,x
    sta SCREEN+$100,x
    sta SCREEN+$200,x
    inx
    bne fill

    ; Distinct colours in every register mode 10 can pick.
    lda #$12
    sta COLPM0
    lda #$24
    sta COLPM1
    lda #$36
    sta COLPM2
    lda #$48
    sta COLPM3
    lda #$5A
    sta COLPF0
    lda #$6C
    sta COLPF1
    lda #$7E
    sta COLPF2
    lda #$92
    sta COLPF3
    lda CFG_COLBK
    sta COLBK

    lda #<dlist
    sta DLISTL
    lda #>dlist
    sta DLISTH

    ; The mode under test, then the display, so the picture starts whole.
    lda CFG_PRIOR
    sta PRIOR
    lda #$22
    sta DMACTL

forever:
    jmp forever

; The OS calls the init address before the run address; there is nothing
; to do there.
init:
    rts

; One screen byte per pair of pixels: nibbles 0-15 in order.
ramp:
    !byte $01, $23, $45, $67, $89, $AB, $CD, $EF

; 24 blank scan lines, then 16 rows of mode F, then wait for vertical blank
; and start again.
dlist:
    !byte $70, $70, $70
    !byte $4F
    !word SCREEN
    !fill 15, $0F
    !byte $41
    !word dlist

; The OS looks here when it finds a cartridge: run address, the zero that
; says a cartridge is present, the option byte ($04: start the cartridge,
; no disk boot), and the init address.
* = $BFFA
    !word start
    !byte $00, $04
    !word init
