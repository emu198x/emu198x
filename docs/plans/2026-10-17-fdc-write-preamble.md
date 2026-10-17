# Preserve a written data mark before the first host byte

Lane: Spectrum launch-hardening. Base: `053679207c9390ca8387d3bcdc0d83d36808a3d8`.

## Cause and boundary

The existing pre-data interval crosses gap/sync/DAM, but the selected mark and
incomplete data CRC are only recorded when a host byte is accepted. A zero-byte
overrun therefore leaves the old mark and CRC. Fuse 1.7.0 `start_write_data`
writes FB/F8 before asserting the first data request; MAME `WRITE_SECTOR_DATA`
writes the mark before popping the first host byte. NEC December 1978 p13 says
Write Deleted Data substitutes the deleted mark at the beginning of the field.
This change preserves that ordering in the existing sector-image model; it does
not claim physical CRC bytes, flux or new timing accuracy.

## Tasks

1. Add public-register zero-byte-overrun regressions for normal-over-deleted and
   deleted-over-normal. Check active export refusal, unchanged IDs/payload and
   bad data CRC after the overrun; prove both fail before the fix.
2. In `crates/nec-upd765a/src/lib.rs`, commit the selected mark and incomplete CRC
   at the end of the existing preamble before raising the first RQM. Check
   readiness and protection again before touching media. Keep Scan unchanged.
3. Run the new tests, controller suite, fmt and targeted Clippy. Retain red/green
   output here. Commit and push the isolated branch for parent integration.

No dependencies, schema changes, timing-constant changes or source-file writes.

## Verification

- Before: both new zero-byte-overrun regressions failed with old `(ST1, ST2)`
  `(0, 64)` and `(0, 0)` rather than `(32, 32)` and `(32, 96)`.
- After: all 99 controller tests pass (55 unit + 44 integration); zero ignored.
  The two new guards cover protection becoming read-only and a missing
  attachment during the preamble, before payload is requested.
- `cargo clippy --locked -p nec-upd765a --all-targets -- -D warnings`,
  `cargo fmt --all --check` and `git diff --check` pass.
- Tests use only private in-memory images. They check all image IDs/payloads,
  neighbouring sectors and heads, and export refusal during the active write.
  No ROM qualification or physical CRC-byte result is claimed.

Commands use Rust 1.99.0 and the locked source dependencies. Raw test and Clippy
outputs are adjacent `*-before.txt`, `*-after.txt` and `*-clippy.txt` files.
The `mise` tracked-config warning records a sandbox-denied bookkeeping symlink;
Cargo itself completed, and before-test exit 101 / after-test exit 0 were read.
