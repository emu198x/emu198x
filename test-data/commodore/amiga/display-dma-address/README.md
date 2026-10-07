# Registered display-address samples

128 rows emitted by the unmodified `custom.cpp::bitplane_rga_ptmod` function
at FS-UAE revision `f362278ccd4c60991caac3b4d240d4a3f751bea2`.
Source SHA-256: `75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5`.

Columns: plane, terminal-modulo flag, beam line, DIWSTRT vertical byte,
FMODE bit-14 selector, captured address, captured signed modulo.

The compiled source sweep covers all eight planes, both flag values, both
line and DIWSTRT parities, and both modulo selectors. The Rust regression
replays each row at all three transfer widths, changes PT/MOD after reservation
and again after addressing, and verifies the retained service descriptor.
These are software-reference address samples; they do not validate the live
DDF reservation sequencer or physical silicon. Primary observations:
`reference/by-system/commodore-amiga/2026-copper-blitter-wake-observations.md`
in the umbrella repository.

Regenerate into a scratch directory with `python3 build-reference.py
/path/to/registered/custom.cpp /private/tmp/display-address-reference`. The
script requires the registered source hash and exactly 128 emitted rows.
Compare its `reference-address.csv` with `registered-ptmod.csv` before
accepting a fixture update. It does not modify the reference source.


`registered-service.csv` retains all 128 rows from the unmodified FS-UAE
bitplane service branch and its lane readers. Columns: reservation FMODE,
service FMODE, address, captured signed modulo, resulting pointer, payload
word count, and four payload words. The harness uses sentinel chip words
1111, 2222, 3333, 4444 and checks its output count before writing the fixture.

The Rust comparison asserts exactly 128 encountered rows, 96 compared rows,
and 32 explicitly retained FMODE-2 producer disagreements. FS-UAE's old
branch uses `fetch64` for mode 2 while its byte increment is four. The Lisa
specification distinguishes mode 2 as two 16-bit CAS transfers. Vendored
WinUAE revision `c32694e338fa5f34977f522eb4898adb069d2e73` instead selects
`fetch32_bpl` for modes 1 and 2; its `custom.cpp` hash is
`1ee355cece1e3cd9a176c70a2452e0f8f53aa44c954e9b851f19df0dd9428634`.
That upstream code was inspected, not run in this harness. The 32 old-FS-UAE
mode-2 rows are disagreements, not accepted goldens. Live native replay checks
mode-2 width/lanes against the existing Lisa model and primary specification;
these native checks are not a new independent page-mode capture.

PT/MOD remain captured in addressing, but service width and lane selection
follow immediate FMODE. After service, the resulting words and width are
retained through the normal Denise RGA stage. Automatic DDF sequencing is
outside these fixture gates. Wider upstream pointer alignment changes also
need a separate investigation; this fixture does not validate those changes.
