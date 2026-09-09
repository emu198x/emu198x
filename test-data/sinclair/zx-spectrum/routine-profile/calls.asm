; Explicit routine extents: main..end_main and work..end_work.
; asm198x --dialect pasmo --debug test-data/sinclair/zx-spectrum/routine-profile/calls.asm -o test-data/sinclair/zx-spectrum/routine-profile/calls.bin
        org $c000
main:   call work
        call work
        halt
end_main:
        defs 9
work:   ld b,2
.loop:  nop
        djnz .loop
        ret
end_work:
        end main
