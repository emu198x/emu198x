# SNES fixed regional owners

Adopted 2026-10-31 under the approved #2144 continuation.

The implemented NTSC and PAL configurations have separate owners over
`common-nintendo-snes`. Super Famicom remains an NTSC profile with its existing
profile/model ID. This does not qualify every PCB or chip population, nor add
CIC execution. OpenSFC's original board reconstruction documents SHVC-CPU-01
and SNS-CPU-GPM-01 reuse across NTSC-U/J; a market name is not a motherboard.
See the primary record in reference/by-system/nintendo-snes/
opensfc-regional-board-scope.md and OpenSFC README commit 6574450b.

This narrow override of runtime-internal-shape.md keeps the existing shared
runtime concerns, adding a closed `machine.rs` dispatcher rather than a new
generic runtime axis. Fixed bindings validate restored region/PPU and memory
before installation, and never expose mutable replacement of raw state.

Version-1 machine bytes remain unchanged. Runtime caches are rebuilt from the
embedded cartridge and IPL; hard reset retains the same cartridge's SRAM.
Empty restores clear obsolete caches. The firmware query distinguishes a
non-built-in program by its bytes; explicitly supplying the clean-room program
does not make it original Nintendo firmware. v1 carries bytes, not input-path
provenance. Specific board revisions, peripheral electrical differences and
lockout validation remain future qualification work.

On 2026-11-01 Steve approved v2 envelopes carrying an independent optional
64-byte IPL. Absence means the clean-room program. Loaded v1 saves retain an
exact reader because their machine embeds the IPL; blank v1 saves are rejected
because they omitted it. V2 rejects mismatched envelope/machine firmware before
installation. Original v1 machine bytes and six continuation captures remain
unchanged; a test-only legacy envelope encoder compares their complete output.
