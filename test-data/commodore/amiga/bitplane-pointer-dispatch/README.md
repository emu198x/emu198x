# Deliver every implemented bitplane pointer half

All three machine register dispatch ranges ended at `$0F5`, so CPU and
Copper writes to BPL6PTL (`$0F6`) were silently lost. The A1200 also dropped
both halves of AGA pointers 7 and 8 (`$0F8..$0FE`). The corrected ranges end
at `$0F6` for OCS/ECS and `$0FE` for AGA. Existing Agnus handlers, timing,
word alignment and save-state layout are unchanged.

## Sources and failing evidence

The original *Amiga Hardware Reference Manual* (1985), Appendix A register
map, lists BPL6PTL at `$0F6`; its Copper example writes that low half. The
umbrella reference library retains the primary extract at
`reference/by-system/commodore-amiga/1985-amiga-hardware-reference-manual.txt`
(lines 12145–12146 and 3923–3924). WinUAE's `custom.cpp` write dispatch
(cases `$0E0..$0FE`, lines 7499–7514 in the reviewed local reference)
provides the independent implementation precedent for all eight pairs.

`crates/runtime-commodore-amiga/tests/bitplane_pointer_dispatch.rs` executes
real CPU MOVE.W instructions and separately fetches actual Copper lists on
OCS, ECS and AGA. All six tests failed before the change, first at BPL6's
low half. All six pass afterwards. They check every pair, odd-address
masking, and OCS/ECS ignoring the unimplemented seventh/eighth pairs.
`red.log`, `green.log` and `clippy.log` preserve those results.

## Banshee diagnosis

The old catalogue revision `1cead4e8` and pre-fix `f41195ed` have identical
RAM throughout the displayed `$100000..$12FFFF` region at the POWERUPS
waypoint. The Copper list at `$5C254..$5C290` writes all eight pointers.
Before the fix BPL6 retains the wrong low half and BPL7/BPL8 advance far
outside their programmed display addresses. The resulting wrong fetches
produce the black streaks and duplicate text.

`baseline/` and `before-fix/` retain the completed framebuffer and register
observations from those two revisions. `restored-after-fix/` restores the
same failing version-62 snapshot into the corrected machine and runs two
fields: the eight pointers are contiguous again and the streaks/duplicate
text disappear. This is a causal diagnostic, not a replacement catalogue
capture; the catalogue must also boot from the unchanged media.

Raw guest memory, save states and disk/firmware contents are not included.
The separate assessment catalogue evidence records the full cold-boot run
and identifies any title still awaiting classification. This correction
does not establish full Amiga or title pixel accuracy.

The complete unchanged-media cold boot now produces framebuffer hash
`4ee6197e2dee4e3b`, exactly matching the two-field restored diagnostic.
`cold-boot-after.png` retains that result. Its audio hash is unchanged by
this fix (`49aa27d3f82af1ab`), and the normal catalogue snapshot check passes.
All ten catalogue entries pass snapshot replay; State of the Art still
needs reference classification before its expected image can be changed.
