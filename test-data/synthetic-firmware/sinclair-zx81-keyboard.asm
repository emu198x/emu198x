; Synthetic ZX81 firmware that shows which keyboard row is being pressed, so
; CI can prove the keyboard reaches a running machine without Sinclair's ROM.
;
; #1041 was the ZX81 ignoring every key. Nothing was wrong with the keyboard
; code — `read_keyboard` and the matrix had unit tests throughout and all of
; them passed. What was wrong is that the ROM never reached the state where it
; processes input, because the display was drawn for it at frame boundaries
; instead of by the CPU executing the display file (#1032). Only an end-to-end
; test could see that, and the one that exists needs Sinclair's ROM, which
; cannot be staged on a runner. This image is the part that can.
;
; ## What it draws
;
; One character row per keyboard row: press anything in row N and character
; row N goes solid. Rows 8-23 stay blank.
;
; Row *selection* is the thing worth testing, not merely "a key registered".
; A machine that returned the same byte for every row — the shape of failure a
; broken `RLC B` walk would give — would light every row at once, or the wrong
; one, and a test that only counted ink would pass it. So the assertion is
; which band is lit, and the count of lit bands.
;
; ## Structure
;
; The display half is `sinclair-zx81-display.asm` unchanged: the bit-6 NOP
; forcing, a display file written out as 192 physical rows so the line handler
; can be a bare RETN, and the software vertical sync that ends the field. Read
; that file for why each of those is the way it is; this one only changes what
; the display file *contains*, and rebuilds it once a field.
;
; ## Why it only repaints when the keys change
;
; Rewriting the eight character rows is 2,112 stores, about 27,000 T-states —
; a hundred and thirty scan lines. Doing that every field pushes the field
; past the 312 lines the ULA's clock free-runs to, its end-of-field backstop
; fires before the software sync, and the picture stops landing in the window
; at all. The first version of this did exactly that and drew nothing.
;
; So the scan produces a byte, one bit per row, and the repaint happens only
; when that byte differs from the last field's. The common case — nothing
; changed — costs about five hundred T-states. Real software does not redraw
; the screen every frame either.
;
; ## Where the scan happens
;
; In the vertical interval, with the NMI generator off — there is no line
; clock then, so the CPU is free. It cannot happen during the display, where
; the CPU is busy being the video hardware, and it must not happen with the
; generator on, because NMI would arrive somewhere the line handler does not
; expect to be entered from.
;
; The scan's own `IN`s have A0 low, so each would start a vertical sync. That
; is harmless here: the generator-on `OUT` at the top of the field releases it
; after a few hundred T-states, far short of a field sync, and the ULA reads a
; short pulse as a line sync — which resets the character-row counter to zero
; exactly where a field wants it.
;
; Assemble with:
;     asm198x asm --dialect pasmo --cpu z80 sinclair-zx81-keyboard.asm -o out.bin

DFILE        equ $4100          ; ROWS rows of COLS glyphs, each NEWLINE-ended
TOPB         equ $5A00          ; TOP_LINES NEWLINEs, then a RET
BOTB         equ $5A80          ; BOT_LINES NEWLINEs, then a RET
PREV         equ $5B00          ; the last scan, so an unchanged one costs nothing
ROWS         equ 192            ; 24 character rows of 8 scan lines, written out
COLS         equ 32
KBROWS       equ 8
TOP_LINES    equ 55             ; 56 less the display file's own blank first row
BOT_LINES    equ 40
NEWLINE      equ $76            ; and the HALT opcode, which is the point
SOLID        equ $80            ; glyph 0 inverted: blank bitmap, every pixel set
BLANK        equ $00            ; glyph 0 upright: blank bitmap, no pixel set
ENDMARK      equ $C9            ; RET. Bit 6 set, so the ULA lets it execute
                                ; and it ends the block by returning.

             org $0000
             jp start

; ---------------------------------------------------------------------------
; A display line has ended. The interrupted PC is already the next row's first
; byte, so there is nothing to do but go back to it.
             org $0066
nmi:         nop                ; PAD: four T-states is eight pixels of
             nop                ; horizontal position, measured against the
             nop                ; rendered frame
             retn

; ---------------------------------------------------------------------------
             org $0080
start:       di
             ld sp,$7000
             ld a,$1E           ; character bitmaps at $1E00, where Sinclair's
             ld i,a             ; ROM keeps them. This ROM is zero there, so
                                ; glyph 0 is blank and SOLID is it inverted.

             ld hl,TOPB
             ld b,TOP_LINES
             call fill_blank
             ld hl,BOTB
             ld b,BOT_LINES
             call fill_blank

             ld hl,DFILE        ; a blank leading row: the first scan line of a
             ld (hl),NEWLINE    ; `run` is entered from the caller rather than
             inc hl             ; from the line sync, so it is mistimed by the
                                ; call overhead and lands to the right of the
                                ; rest. Spending it on a row with nothing in it
                                ; makes that invisible.
             ld b,ROWS          ; every row blank to begin with; the scan fills
build_row:   ld c,COLS          ; the first eight in
build_col:   ld (hl),BLANK
             inc hl
             dec c
             jr nz,build_col
             ld (hl),NEWLINE
             inc hl
             djnz build_row
             ld (hl),ENDMARK

             ld a,$FF           ; no real scan can match this, so the first
             ld (PREV),a        ; field always paints

frame:       call scan_keys     ; D = one bit per row, row 0 in bit 7
             ld a,(PREV)
             cp d
             jr z,no_repaint    ; unchanged: leave the display file alone
             ld a,d
             ld (PREV),a
             call paint_rows

no_repaint:  out ($FE),a        ; bit 0 low: NMI generator on. SLOW mode, and
                                ; with it the line clock and the display.
             ld hl,TOPB+$8000   ; A15 set — the display-file half of the map
             call run
             ld hl,DFILE+$8000
             call run
             ld hl,BOTB+$8000
             call run

             out ($FD),a        ; bit 1 low: generator off, for the interval
             in a,($FE)         ; A0 low starts the vertical sync
             ld b,100           ; held long enough to read as a field sync
sync_wait:   djnz sync_wait     ; rather than a line one
             out ($FF),a        ; any OUT releases it, and the field ends here
             jr frame

; ---------------------------------------------------------------------------
; One bit per keyboard row: set if any key in it is down. Row 0 lands in bit 7.
;
; `IN A,(C)` puts B on A8-A15, so walking B with RLC selects each row in turn —
; active low, one bit per row. Bits 0-4 of the result are the five keys, also
; active low; bits 5-7 are not keyboard and are masked off.
scan_keys:   ld bc,$FEFE        ; C = the port, B = row 0 selected
             ld d,0
             ld e,KBROWS
sk_row:      in a,(c)
             cpl                ; keys are active low: 1 now means pressed
             and $1F            ; the five key bits, and nothing else
             jr z,sk_none
             scf
             jr sk_shift
sk_none:     or a               ; A is zero here, so this clears carry
sk_shift:    rl d               ; shift the row's answer in
             rlc b              ; select the next row
             dec e
             jr nz,sk_row
             ret

; Fill character row N of the display file from bit 7 of D downwards, for the
; first eight rows. Each character row is eight physical rows of COLS glyphs.
paint_rows:  ld hl,DFILE+1      ; past the blank leading row
             ld c,KBROWS
pr_charrow:  rl d               ; carry = this row's flag, row 0 first
             ld a,BLANK
             jr nc,pr_have
             ld a,SOLID
pr_have:     ld b,8             ; scan lines in a character row
pr_line:     push bc
             ld b,COLS
pr_col:      ld (hl),a
             inc hl
             djnz pr_col
             inc hl             ; step over the row's NEWLINE, already written
             pop bc
             djnz pr_line
             dec c
             jr nz,pr_charrow
             ret

; ---------------------------------------------------------------------------
; B blank scan lines at HL: a NEWLINE each, then the marker that ends the run.
fill_blank:  ld (hl),NEWLINE
             inc hl
             djnz fill_blank
             ld (hl),ENDMARK
             ret

; Run the display file at HL. Returns when the CPU reaches its ENDMARK.
run:         jp (hl)
