; Spectrum 128 banked program — ILLUSTRATIVE SOURCE for the hand-authored
; banked fixture (spectrum128-banked.debug198x). No asm198x dialect emits
; banked records yet (sjasmplus DEVICE/PAGE banking is not implemented), so
; both this source and its sidecar are authored by hand per plan R10: the
; fixture validates the banked *shape* — Space::Paged records, cross-bank
; lookups — ahead of any emission path populating it.
;
; The program's shape: two routines, one in RAM bank 1 and one in RAM bank 3,
; both paged into slot 3 ($C000-$FFFF) and both sitting at the same in-slot
; offset — so the same CPU address ($C010) names a different symbol and a
; different source line depending on which bank is paged in.
;
;   bank 1 (slot 3):
;     $C010  draw:   ld a,4        ; line 5 of the notional per-bank source
;   bank 3 (slot 3):
;     $C010  music:  ld a,7        ; line 12
