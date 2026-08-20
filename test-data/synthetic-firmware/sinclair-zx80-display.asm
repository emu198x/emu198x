; Synthetic ZX80 firmware that generates its picture the way the hardware
; does, for CI evidence that display generation works without shipping
; Sinclair's ROM.
;
; The ZX80 has no video chip. The CPU executes through the display file
; while the discrete logic forces NOPs and turns the fetched bytes into
; pixels, so firmware that merely writes a display file and spins renders
; nothing at all — correctly. This runs a real display routine.
;
; ## Structure, and why it copies the ROM's
;
; The interrupt handler does the work: it counts scanlines in C and rows in
; B, and jumps back into the display file itself. The caller only arms the
; first line and waits for RET.
;
; That is the ROM's design and the timing is the reason. The horizontal
; position of the picture is set by how long the machine takes to get from
; the interrupt back to the first character fetch, because that is the gap
; the beam spends off the left of the visible area. A call/return wrapper
; around each scanline costs about twice as long and pushes the picture off
; the right-hand edge. Measured from the `HALT` release, the same-row path
; below is:
;
; ## Where the picture sits
;
; The horizontal position is set by how long the machine takes to get from
; the interrupt back to the first character fetch, because that gap is what
; the beam spends off the left of the visible area. `FIRST_CHAR_TSTATE` in
; machine-sinclair-zx80 is calibrated to the real ROM's handler, so a
; routine that dawdles pushes the picture right and one that hurries clips
; characters off the left edge. Both were measured here, in that order.
;
; Two consequences worth keeping:
;
;   * `A` is loaded with `R_ARM` once, by `display`, and the handler only
;     does `LD R,A`. Reloading it per line costs 7 T — fourteen pixels.
;   * The same-row and new-row paths must cost the same, or every eighth
;     scanline steps sideways. They are within a couple of T-states here.
;
; A call/return wrapper around each scanline costs about twice as long as
; the whole handler and puts the picture off the right-hand edge entirely,
; which is what the first attempt at this did.
;
; ## Arming R
;
; `/INT` is wired to A6 of the refresh address, so a line ends when R counts
; A6 low. From `LD R,A`, one M1 for `JP (HL)`, 32 for the row's characters
; and one for the NEWLINE's own fetch is 34; loading 94 puts R at 128 on
; that last fetch, wrapping the low seven bits to zero and pulling A6 down
; while the CPU is halted. The border row reaches its `HALT` 32 M1 cycles
; earlier and idles there for exactly those 32 cycles, so every line is the
; same length whether it carries characters or not.
;
; Assemble with:  asm198x asm --dialect pasmo --cpu z80 sinclair-zx80-display.asm

R_ARM       equ $5E             ; 94: see "Arming R" above
DFILE       equ $4100           ; 24 rows of 32 glyphs + NEWLINE
BORDER      equ $4000           ; a lone NEWLINE, reused for every border line
ROWS        equ 24
COLS        equ 32
BORDER_LINES equ 47             ; calibrated, not derived: it places the top of
                                ; the picture at the top of the visible window.
                                ; The 3-bit line counter goes out of phase at
                                ; a count that is not a multiple of 8, which
                                ; costs nothing here because every glyph is
                                ; solid — inverted blank sets every pixel of
                                ; every row.
NEWLINE     equ $76
SOLID       equ $80             ; glyph 0 inverted: blank bitmap, every pixel set

            org $0000
            jp start

; ---------------------------------------------------------------------------
; A display line has ended: R counted A6 low and released the HALT.
            org $0038
irq:        dec c               ; scanlines left in this character row
            jp nz,same_row
            pop hl              ; the interrupted PC is the *next* row's first
                                ; byte — the display file is contiguous, and
                                ; the CPU stopped just past this row's NEWLINE
            dec b               ; rows left
            ret z               ; all drawn: back to whoever called `display`
            set 3,c             ; C was 0; make it 8 for the next row
arm:        ld r,a              ; A already holds R_ARM: `display` set it and
                                ; nothing in this loop touches A. Reloading it
                                ; here costs 7 T-states on every line, which is
                                ; 14 pixels of horizontal offset — see "Where
                                ; the picture sits".
            ei
            jp (hl)
same_row:   pop de              ; discard the interrupted PC and draw the same
                                ; row again, one pixel row further down
            ret z               ; never taken — `dec c` left NZ, or we would
                                ; not be here. Five T-states of padding, which
                                ; is what this path owes the other one: the
                                ; new-row path spends DEC B, RET Z and SET 3,C
                                ; that this one does not, and without the debt
                                ; repaid every eighth scanline starts ten
                                ; pixels left of the rest and loses two
                                ; characters off the edge.
            jr arm

; ---------------------------------------------------------------------------
            org $0050
start:      di
            ld sp,$4800
            im 1
            ld a,$0E            ; character bitmaps live at $0E00 in this ROM,
            ld i,a              ; as they do in Sinclair's

            ld hl,BORDER        ; one NEWLINE, drawn 56 times for the top border
            ld (hl),NEWLINE

            ld hl,DFILE         ; a blank row first: the first scanline of a
            ld (hl),NEWLINE     ; `display` call is entered from the caller
            inc hl              ; rather than from the interrupt, so it is
                                ; mistimed by the call overhead and lands to
                                ; the right of the rest. Spending it on a row
                                ; with nothing in it makes that invisible.
            ld b,ROWS
build_row:  ld c,COLS
build_col:  ld (hl),SOLID
            inc hl
            dec c
            jr nz,build_col
            ld (hl),NEWLINE
            inc hl
            djnz build_row

; Each field: sync, top border, then the character rows.
frame:      in a,($FE)          ; A0 low reads the keyboard *and* starts the
                                ; vertical sync — one port, two jobs
            ld b,20
sync_wait:  djnz sync_wait
            out ($FF),a         ; any OUT ends the sync; the field starts here

            ld hl,BORDER+$8000  ; A15 set: the display-file half of the map
            ld b,1
            ld c,BORDER_LINES
            call display

            ld hl,DFILE+$8000
            ld b,ROWS+1         ; +1 for the blank leading row
            ld c,8
            call display
            jr frame

; Draw B rows of C scanlines starting at HL. Returns when B reaches zero.
display:    ld a,R_ARM          ; loaded once; the handler reuses it every line
            ld r,a
            ei
            jp (hl)
