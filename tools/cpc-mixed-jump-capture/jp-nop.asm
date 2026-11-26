; Amstrad CPC bus-capture stimulus for the mixed JP/NOP timing question.
; Load at &8000 and enter from BASIC with CALL &8000 (interrupts enabled).
; Runs 256 short passes, restores BC, reenables interrupts and returns.
        org $8000
        di
        push bc
        ld b,0          ; DJNZ with B=0 gives 256 passes.
pass:
        nop
        nop
        nop
        nop
before:
        nop
jump:
        jp target
        db $00          ; Never fetched: the JP is not a fall-through.
target:
        nop
after:
        nop
        nop
        nop
        djnz pass
        pop bc
        ei
        ret
