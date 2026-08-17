; Border-colour walk — the Debug198x importer's end-to-end fixture.
;
; Deliberately shaped like a Code198x lesson build: an absolutely-located
; C64 program with a named constant, two labels, a loop, and one byte of
; data. Small enough to disassemble whole in a test, but with a distinct
; line to set a source-line breakpoint on.

* = $c000

border  = $d020

start:
        lda #$00
        sta counter
loop:
        inc counter
        lda counter
        sta border
        cmp #$05
        bne loop
done:
        rts
counter:
        !byte 0
