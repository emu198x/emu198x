# Bounded +3 Format Track

Lane: Spectrum launch-hardening; next controller slice of Emu #45. The user
approved continuing from the partial-SAVE campaign to Format Track, followed
by a separate ROM FORMAT campaign. No new dependency or runtime interface.

1. Reproduce the missing six-byte Format Track command (`0x4d`) as a failing
   controller test. Current main is `69f80d939`. Read NEC December 1978 p.14
   and command/status tables, Fuse `start_write_id`/`upd_fdc_write_data`, and
   MAME `WRITE_TRACK_*`. Archive controllers are precedent only, not ports.
2. In `crates/nec-upd765a/src/lib.rs`, append the command enum variant (retain
   existing postcard tags), consume SC four-byte CHRN records, preserve their
   order and fill each data field using command N and D. Physical track/head
   comes from seek/unit selection, not supplied ID fields. Reuse serialized
   buffers/cursor/deadline; no struct layout or snapshot version change.
3. Keep the existing sector-level model. Commit each complete ID record as a
   filled sector, matching Fuse's ID-record transaction granularity. Preserve
   completed sectors on overrun/reset/eject; do not claim flux-level partial
   ID/data-field fidelity or exact surviving old-track fragments. Other tracks
   and heads must remain unchanged. Successful format removes the old tail.
4. Tick the existing master-loop-derived deadline: FM/MFM byte periods,
   pre-index/ID sync, per-sector fill/gap intervals, final gap to one 300-rpm
   revolution. No synthetic rotational phase or head-load accuracy claim.
   Reject zero count, excessive size or a layout exceeding that revolution
   before mutation. Fail protected, missing-media and absent physical tracks.
5. Tests: direction/readiness, nonsequential CHRN/read-ID/read-data agreement,
   independent fill expectations, physical selection, protection/readiness,
   malformed bounds, overrun/reset/eject and unchanged other tracks. In
   `crates/runtime-sinclair-zx-spectrum/src/snapshot.rs`, verify completed and
   mid-ID/gap format states restore and continue exactly, with source cache
   unchanged. Record old-reader limitation for the new command tag.
6. Run controller/parser, Spectrum runtime and affected machine regressions,
   format and Clippy. Review, push/merge the owning PR; ship a release through
   the existing release workflow when ready. Update #45 and the umbrella
   progress view, keeping ROM FORMAT qualification visibly separate.
7. Remove task worktree `output/worktrees/emu-plus3-format` and its build cache
   after verifying retained commits/evidence. No proprietary ROM/state files
   may be published.

## Implemented boundary

The oldest archived controller returns a successful FORMAT result without
changing media; the newer archive has no FORMAT path. Neither is ported.
The new implementation records complete sectors at ID-record granularity,
like Fuse. That is an explicit limitation: it does not reproduce damaged ID
fields or flux fragments from a reset during an encoded field. The tail of an
old track is removed on successful completion. Overfull/zero-count/oversized
layouts fail before mutation with abnormal ND, an explicit model boundary
rather than a claim about every silicon edge case.

The accepted layouts fit one fixed 300-rpm revolution at 250-kbit/s MFM
(6,250 encoded bytes) or its FM counterpart (3,125 bytes). The initial angular
position and head-load latency remain unmodelled. Per-byte rounding uses the
existing CPU-tick conversion. Servicing ID requests later within their windows
does not stretch the encoded interval; tests cover three service delays.

Snapshot v5 keeps its existing field layout and existing enum tags. The new
command is appended. A synthetic controller captured from released 0.33.0
round-trips byte-identically and its interrupted Write Data resumes correctly.
Old binaries cannot understand a snapshot retaining the new FormatTrack tag;
new binaries can read the old v5 layout. This is forward feature availability,
not rejection or migration of existing saves. The native runtime exercises
five format checkpoints and completed-state restore, with cached source bytes
unchanged.

The original six-byte command test failed (`left: 16`, `right: 48`), proving
that no execution phase existed. With the implementation, 207 affected library
tests pass, including 51 controller and 108 Spectrum runtime tests. Targeted
all-target Clippy and workspace formatting pass. ROM FORMAT/save/catalogue/load,
independent execution, export/reopen and whole-controller fidelity remain
separate qualification; this commit does not close #45.

The four targeted integration suites (`snapshot_roundtrip`, `variants`,
`boot_invariants`, `format_matrix`) add 110 passes and eight pre-existing
firmware-dependent ignored cases; none is counted as a pass. The normal
`cargo build -p emu198x-spectrum --locked` path succeeds. Existing dependency
updates, including the open muda update PR, remain outside this controller slice.
