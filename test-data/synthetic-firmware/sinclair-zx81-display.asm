; Synthetic ZX81 firmware that generates its picture the way the hardware
; does, for CI evidence that display generation works without shipping
; Sinclair's ROM — which, unlike the Spectrum's, Amstrad never bought and
; cannot be redistributed.
;
; The ZX81 has no frame buffer. The CPU executes through the display file
; with bit 15 of the address set; the ULA forces every byte with bit 6 clear
; to a NOP, latches it as a character code, and shifts eight pixels out on
; the refresh cycle that follows. Firmware that merely writes a display file
; and spins therefore renders nothing at all — correctly — which is what
; this image used to do, and why its test could pass against an emulator
; with no display generation in it (#1032).
;
; ## Why this is NMI-driven where the ZX80's is INT-driven
;
; The clearest difference between the two machines. The ZX80 has no line
; clock: /INT is wired to A6 of the refresh address, the line ends when R
; counts A6 low, and the ROM arms R to choose the length —
; `sinclair-zx80-display.asm` does exactly that. The ZX81's ULA generates the
; line clock itself and presents it as /NMI once per line whenever the
; generator is enabled, which is what SLOW mode is. So this runs with
; interrupts disabled and lets NMI end each line.
;
; ## Why the display file is 192 rows and not 24
;
; Sinclair's ROM keeps 24 rows and re-enters each one eight times, counting
; the repeats itself. Doing that here needs a handler that branches on the
; scan-line count, and the two paths through it cost different numbers of
; T-states — 15 apart, in the version of this that had one. That difference
; is horizontal position: every eighth scan line started 30 pixels along from
; the other seven and the picture came out with a step in it.
;
; Writing the rows out physically removes the branch and the bookkeeping with
; it. The handler becomes a bare RETN: the interrupted PC is already the next
; row's first byte, because the display file is contiguous and the CPU
; stopped just past the NEWLINE. Every line then costs exactly the same, and
; the picture has no step in it.
;
; The ULA's own three-bit COUNT still selects which pixel row of the glyph is
; shown, so the eight repeats do differ on screen — that part is hardware and
; is not being stood in for here.
;
; ## Where the picture sits
;
; Horizontally, by how long the machine takes to get from the line sync back
; to the first character fetch: that gap is what the beam spends left of the
; visible area. `FIRST_CHAR_TSTATE` in machine-sinclair-zx81 is measured
; against Sinclair's ROM, so this routine has to arrive at the same point in
; the line. PAD below is what tunes it, and it was measured, not reasoned.
;
; Vertically, by the line count before the first character row: 55 border
; lines plus the display file's own blank first one, and a set blanks the
; first 24 of the frame, so the picture lands 32 rows into the window.
;
; The field also has to be comfortably shorter than the 312 lines the ULA's
; clock free-runs to, or its end-of-field backstop fires before the software
; sync below and the picture wraps around the frame boundary in two pieces.
; 55 + 1 + 192 + 40 is 288 lines, which leaves the margin.
;
; Assemble with:
;     asm198x asm --dialect pasmo --cpu z80 sinclair-zx81-display.asm -o out.bin

; Laid out clear of the system variables at $4000-$407C. Nothing here writes
; D_FILE at $400C, and the test asserts that it is still zero: the renderer
; this replaced read the picture through that pointer, so a full screen with
; it unset is the proof that the picture came off the bus instead. Putting a
; border block at $4000 would have filled it with NEWLINEs by accident, and
; the assertion caught exactly that.
DFILE        equ $4100          ; ROWS rows of COLS glyphs, each NEWLINE-ended
TOPB         equ $5A00          ; TOP_LINES NEWLINEs, then a RET
BOTB         equ $5A80          ; BOT_LINES NEWLINEs, then a RET
ROWS         equ 192            ; 24 character rows of 8 scan lines, written out
COLS         equ 32
TOP_LINES    equ 55            ; 56 less the display file's own blank first row
BOT_LINES    equ 40
NEWLINE      equ $76            ; and the HALT opcode, which is the point
SOLID        equ $80            ; glyph 0 inverted: blank bitmap, every pixel set
ENDMARK      equ $C9            ; RET. Bit 6 set, so the ULA lets it execute
                                ; and it ends the block by returning.

             org $0000
             jp start

; ---------------------------------------------------------------------------
; A display line has ended: the row's NEWLINE is a HALT and the ULA's line
; sync released it. The interrupted PC is already the next row's first byte,
; so there is nothing to do but go back to it.
             org $0066
nmi:         nop                ; PAD: four T-states is eight pixels of
             nop                ; horizontal position. Three of them put the
             nop                ; picture's left edge on the border, measured
             retn               ; against the rendered frame.

; ---------------------------------------------------------------------------
             org $0080
start:       di                 ; NMI is the only interrupt this uses. /INT is
                                ; wired to A6 of the refresh address and would
                                ; otherwise fire in the middle of a row.
             ld sp,$7000
             ld a,$1E           ; character bitmaps at $1E00, where Sinclair's
             ld i,a             ; ROM keeps them. This ROM is zero there, so
                                ; glyph 0 is blank and SOLID is it inverted — a
                                ; glyph already solid in ROM would light the
                                ; screen with the CPU switched off.

             ld hl,TOPB         ; the two borders: a run of NEWLINEs, which are
             ld b,TOP_LINES     ; HALTs, so each one is a blank scan line
             call fill_blank
             ld hl,BOTB
             ld b,BOT_LINES
             call fill_blank

             ld hl,DFILE        ; a blank leading row: the first scan line of a
             ld (hl),NEWLINE    ; `run` is entered from the caller rather than
             inc hl             ; from the line sync, so it is mistimed by the
                                ; call overhead and lands to the right of the
                                ; rest. Spending it on a row with nothing in it
                                ; makes that invisible — the ZX80's routine
                                ; does the same for the same reason.
             ld b,ROWS
build_row:   ld c,COLS
build_col:   ld (hl),SOLID
             inc hl
             dec c
             jr nz,build_col
             ld (hl),NEWLINE
             inc hl
             djnz build_row
             ld (hl),ENDMARK

; Each field: the picture, then the vertical sync that ends it.
frame:       out ($FE),a        ; bit 0 low: NMI generator on. SLOW mode, and
                                ; with it the line clock and the display.
             ld hl,TOPB+$8000   ; A15 set — the display-file half of the map
             call run
             ld hl,DFILE+$8000
             call run
             ld hl,BOTB+$8000
             call run

             out ($FD),a        ; bit 1 low: generator off. No line clock and
                                ; no display, which is what the vertical
                                ; interval wants.
             in a,($FE)         ; A0 low reads the keyboard *and* starts the
                                ; vertical sync: one port, two jobs. The ULA
                                ; only takes it as a sync with the generator
                                ; off, which is why the order matters.
             ld b,100           ; held long enough to read as a field sync
sync_wait:   djnz sync_wait     ; rather than a line one — length is the only
                                ; thing that tells them apart
             out ($FF),a        ; any OUT releases it, and the field ends here
             jr frame

; ---------------------------------------------------------------------------
; B blank scan lines at HL: a NEWLINE each, then the marker that ends the run.
fill_blank:  ld (hl),NEWLINE
             inc hl
             djnz fill_blank
             ld (hl),ENDMARK
             ret

; Run the display file at HL. Returns when the CPU reaches its ENDMARK.
run:         jp (hl)
