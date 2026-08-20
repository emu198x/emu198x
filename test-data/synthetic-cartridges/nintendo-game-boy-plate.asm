; The Emu198x divider plate for the Game Boy.
;
; A Game Boy will not start without a cartridge, and no commercial ROM can go
; on a public runner — so "does it boot at all" was the claim CI could least
; often check. This cartridge is ours from source, which is what makes it
; committable, and it draws through the real video path rather than poking a
; framebuffer, which is what makes a pass mean something.
;
; ## The mark
;
; `198x/decisions/family-visual-identity.md` makes the mark a divider plate:
; two cells with a full-height rule between them, the prefix cell filled and
; the `198x` cell never varying, framed in a constant house brown. It offers a
; **filled** rendering for "where the plate must hold on its own" — which a
; boot screen is — and a **typed** one for terminal banners.
;
; This is the filled plate, drawn as a bitmap and cut into tiles by the
; builder. An earlier version took the typed rendering on the grounds that a
; tile grid is a terminal; it is not, it is a bitmap display, and the typed
; form was the easier thing rather than the right one.
;
; The one honest departure: the prefix cell should carry Emu198x's `#0d4a7d`,
; and four greys cannot. It takes the darker of the two mid tones, so the cell
; still reads as filled against the paper-toned `198x` cell. The colour axis
; is not faked in a palette that cannot hold it.
;
; The header carries no Nintendo logo. The DMG boot ROM checks for one and
; refuses to hand over otherwise, but this core has no boot ROM and starts at
; $0100, so the bytes are not needed. If boot-ROM emulation ever lands that
; check returns, and the header becomes a question about someone else's
; trademark rather than a technical one.
;
; Tiles and map are appended by build-synthetic-cartridges.py, which draws the
; plate. Keeping the picture in the builder and the program here means the
; artwork can be redrawn without anyone editing Z80.

DEF rLCDC EQU $40
DEF rBGP  EQU $47

SECTION "Entry", ROM0[$0100]
    nop
    jp Start

SECTION "Main", ROM0[$0150]
Start:
    di
    ; VRAM is only safe to write with the LCD off. It is off at reset here,
    ; but a cartridge that assumes its entry state breaks the day that state
    ; is made more faithful.
    xor a
    ldh [rLCDC], a

    ld hl, $8000
    ld de, Tiles
    ld bc, TilesEnd - Tiles
.tiles:
    ld a, [de]
    ld [hl+], a
    inc de
    dec bc
    ld a, b
    or c
    jr nz, .tiles

    ; Blank the map: 32x32 entries, of which 20x18 are visible.
    ld hl, $9800
    ld bc, $0400
.clear:
    xor a
    ld [hl+], a
    dec bc
    ld a, b
    or c
    jr nz, .clear

    ; The plate is PLATE_W x PLATE_H tiles, placed by the builder's own
    ; arithmetic so the picture and its position stay in one place.
    ld de, Plate
    ld b, PLATE_H
    ld hl, $9800 + PLATE_ROW * 32 + PLATE_COL
.row:
    push hl
    ld c, PLATE_W
.cell:
    ld a, [de]
    ld [hl+], a
    inc de
    dec c
    jr nz, .cell
    pop hl
    ; Next map row: 32 entries on.
    ld a, l
    add a, 32
    ld l, a
    ld a, h
    adc a, 0
    ld h, a
    dec b
    jr nz, .row

    ld a, %11100100
    ldh [rBGP], a
    ; LCD on, BG on, tiles at $8000, map at $9800.
    ld a, %10010001
    ldh [rLCDC], a

.spin:
    halt
    jr .spin
