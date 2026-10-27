# Second small-issue fix batch

Lane: engineering-frontier validation and host reliability.
Base: d196c6431ecfbfde73f1a96319658c6e1dc53cde. Use the isolated
fix/small-issue-sweep-2 worktree; preserve the primary checkout and other tasks.

1. **ColecoVision media validation (#233).** Inspect the held BIOS/header
   reference and reference loader. Add MediaSet-path regressions for both
   signatures, short/random inputs and rejection with a live mounted cartridge.
   Validate new media in crates/runtime-coleco-colecovision/src/runtime.rs before
   rebuilding; document the existing raw insertion helper as intentionally raw.
   Preserve its signature and saved-state compatibility. Verify owning runtime
   tests, strict Clippy, and an original cartridge against the original BIOS.
2. **BBC missing teletext font (#2148).** Verify the active video-mode/font
   state in crates/machine-acorn-bbc-micro/src/lib.rs and the final report in
   crates/emu198x-acorn-bbc-micro/src/app.rs. Add a read-only missing-font signal
   derived from the live machine, and an explicit report warning when teletext
   is selected without its font. Keep firmware optional; bitmap modes need no
   font. Cover supplied/missing font, teletext/bitmap and snapshot continuation,
   then run owning tests/Clippy and bounded original-MOS controls.
3. Assess further small fixes only after these regressions pass. No timing,
   CPU, snapshot-schema, dependency or architecture changes are planned.

Each fix is independently verified and committed. Retain before/after evidence
beside this plan, push the batch, require all CI checks to finish successfully
before merging, verify issue closures, and clean task-owned files/processes.

## Results

ColecoVision: the malformed-media regression failed on the old loader while
both signature controls and legacy raw-snapshot control passed. New loads now
validate the 36-byte fixed header and either BIOS signature before replacing
anything, including multi-image requests. The original BIOS confirms the entry
vector at $800A and the RST/NMI jumps through $800C..$8023; its SHA-256 and
original-guest handoff result are in the verification receipt. This does not
claim mapper or full commercial-game qualification. All 13 runtime tests and
six application integration tests pass, as does the explicitly enabled
original-BIOS test and strict owning Clippy. Three initially failing app tests
used raw headerless fixtures; their valid-header replacements retain all
mapping/variant/reset/source-removal checks at a payload byte.

BBC: the old report regression fails on the missing diagnosis. The new
read-only signal uses the live ULA selection and complete-font extent; it adds
no serialized state. Machine-specific warnings survive final CLI report
assembly. All three current models cover absent/supplied font and bitmap output,
plus restoration of a missing-font snapshot after a font was installed. Four
CLI controls also pass. Original Model B MOS/BASIC runs for 100 frames produce
a warning only without the font, even though the existing blank-frame heuristic
reports nothing. Original fixture hashes and reports are retained beside this
plan. All 95 executed machine/runtime tests and all 11 app tests pass; 12
existing fixture/diagnostic tests remain explicitly ignored in the owning
suites. Strict owning Clippy and formatting pass.

The batch is limited to these two bounded fixes. The remaining PSG stereo,
SNES native-frame stepping, board splits and physical controller work need
larger designs or source qualification; this batch adopts none of those.
