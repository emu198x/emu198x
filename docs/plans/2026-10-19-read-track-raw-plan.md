# Raw-input Read Track CRC comparison (#1860)

Lane: Spectrum launch-hardening. User approved the bounded raw-reference study
and asked for a concrete path to closing operations. Base523d37dd98e4ae168d341b34decc9afc654a7515.
No production controller, API, snapshot or dependency change is planned.

1. Inspect installed MAME media capabilities and vendored format/controller
   sources. Prefer an already supported raw container. HFE is not advertised for
   specpls3 and its inspected loader rejects single-sided drives; MFI is
   advertised and describes lossless magnetic-transition input. Use Python
   stdlib zlib only, not a new dependency or production mastering command.
2. Reuse the #1851 raw Fuse fixtures, event host, Lua direct-port probe and
   result/auditor mechanisms. Add a bounded MFI fixture encoder and independent
   decoder under docs/plans/2026-10-19-read-track-raw-crc/. Prove transition cells,
   missing-clock A1s, payload and isolated CRC mutations survive the container.
3. Run 13 one/two-sector clean/ID/data-error cases twice in installed MAME on
   its unchanged internal3ssdd drive. Verify payload order, seven result bytes,
   recovery, immutable inputs and host/emulated bounds. Add Read ID/Read Data
   controls where necessary to prove ID-only errors reached the controller.
   Earlier Emu/Fuse observations are captured replay, explicitly separate from
   fresh MAME execution. No ROM-loading qualification or upstream Fuse report.
4. Retain versioned observations, exact tools/source/adapter/media identities,
   commands/environment, raw logs and derived readable comparison. Add positive
   and negative auditors; reject missing data, altered cells/CRC/clock marks,
   invented equivalence and any false machine qualification. If a prerequisite
   is missing, report it rather than swapping the drive/configuration silently.
5. Write an operation-closure assessment grounded in current code, current tests
   and remaining acceptance gaps. Separate sector-model functional closure,
   released guest coverage and hardware/timing qualification. Do not relabel
   whole operations complete to make a counter improve. Name finite next work.
6. Verify relevant existing tests/lint and peer claims against code. Push owning
   Emu PR, merge only after checks. Update umbrella visual/progress in its own
   PR. Keep wider issues open. Byte-verify exports/archives and remote refs before
   removing task worktrees/tools/private firmware copies/caches/processes.
