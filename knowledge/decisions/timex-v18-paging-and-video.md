# Timex corrected execution uses v18 saves

Adopted 2026-11-03 for #52. Steve explicitly chose writing v18 and rejecting
Timex v17 instead of retaining a reader with changed continued output.

TC2048, TC2068 and TS2068 write and require version 18. Other Spectrum-family
profiles retain version 17. Check the selected profile's version before
positional payload decoding or mutation. Never treat a rejected v17 Timex
payload as a current state by changing its header in production.

This scopes the version-17 continuation guarantee in
[spectrum-video-pipeline-snapshots.md](spectrum-video-pipeline-snapshots.md).
The original Timex v17 fixtures remain immutable historical evidence of the
old paging/rendering model. They must not be regenerated to make a test pass.
Current v18 tests verify complete uninterrupted/restored video, audio bits,
clocks and states, plus atomic legacy-version and foreign-board rejection.
No missing field is silently defaulted and no initial output is suppressed.

The immediate demonstrated bug selects empty DOCK for F4-selected chunks even
when FF7 selects EXROM, while FF7 alone incorrectly overlays HOME. The TS2068
Technical Manual §§2.1.8.1 and 2.1.13.5 defines their combined decoding; §2.1.8.3
separates HOME video RAM fetches from CPU paging. Correcting these reads boots
the original TC2068/TS2068 ROM pair and permits keyboard-entered BASIC.
Mode 6 also changes from interleaved individual bits with black paper to
alternating eight-pixel primary/secondary columns, complementary paper and
border, with bright/flash off. TS2068 Technical Manual Table 3.2.2-1 and §5.2.3
are the authority. Fuse agrees on column order but uses bright colours;
the explicit manufacturer setting takes precedence. Reserved encodings
3/4/5/7 retain unqualified compatibility output. Shared ULA 0.2.1 carries the
renderer fix without changing serialized fields; SCLD timing metadata must
agree with the board's reattached immutable timing configuration.

The complete original TC2068/TS2068 firmware pair is identified before boot,
then startup banners, keyboard-entered `PRINT 3+4`, native pixels and exact
continued video/audio/clocks/state are checked. Full input hashes and strict
fixture gates prevent another ROM or a missing fixture from passing as proof.
The old TS2068 stripe golden is retained in the docs qualification evidence;
its replacement is accepted only after these independent boot checks.
