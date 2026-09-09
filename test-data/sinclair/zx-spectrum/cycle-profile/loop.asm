; Source-level profiling fixture: 57 T-states before HALT refresh waiting.
; Assemble from the repository root with:
; asm198x --dialect pasmo --debug test-data/sinclair/zx-spectrum/cycle-profile/loop.asm -o test-data/sinclair/zx-spectrum/cycle-profile/loop.bin
        org $c000
start:  ld b,3
loop:   nop
        djnz loop
done:   halt
        end start
