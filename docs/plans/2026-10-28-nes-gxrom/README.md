# Mapper 66 verification

The source fixture is
`crates/machine-nintendo-nes/tests/common/gxrom_fixture.rs::cartridge`.
It is original code and contains no commercial ROM. Build a tiny Rust program
that includes that module and writes `fixture::cartridge()` to the requested
path to feed the same bytes to a reference core. The checked SHA-256 is retained
in `reference-results.json`.

`reference_guest.cpp` links the already-built Mesen core. Compile with C++17,
pthreads, the Mesen root/Core/Utilities include directories, and its core library.
On macOS this core's install name is the relative `MesenCore.dylib`: run the probe
from the library directory (an rpath alone does not resolve that bare name).
No vendor source or core binary is modified. `verify_reference.py --binary
<probe> --rom <generated-rom> --output <new-scratch-directory>` runs bounded
five-second guests in fresh homes; it checks all 256 observed RAM bytes.

The normal 16 PRG/CHR combinations agree under all three Mesen configurations.
With an explicit per-cartridge AND-conflict database entry, all bytes agree
including two deliberately conflicting writes. Mesen's default and explicit
unmasked modes differ at exactly the four conflict-result bytes. The test does
not silently accept those modes as proof of AND behaviour.

The cartridge/mapper baseline had three positive-test failures (`Unsupported
mapper: 66`), while its unsupported-layout controls passed. The CPU-executed
cartridge also failed to load. After implementing the mapper, both pass. A
mutation removing the AND mask makes the CPU guest fail with `[131, 163]`
instead of `[129, 161]`; the original source was restored and reverified.

Affected-package tests reported 264 passed, zero failed and 135 ignored. The new
runtime snapshot test includes both NTSC and PAL. Strict all-target Clippy and
workspace formatting passed. Existing snapshot variants retain their order;
the new GxROM variant is appended. There is no new dependency or change to any
existing mapper's fields.
