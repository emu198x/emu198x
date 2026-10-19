# Read Track field boundaries (#1870)

Base: d92b7ca3828efc438f335a2dbdb337c80cf7f346. This is a bounded controller comparison, not machine qualification.

1. Read the NEC primary description and current Emu, Fuse and MAME paths. Freeze named cases and distinguish actual raw fields from sector-image representations. Record unresolved expectations rather than borrowing an oracle silently.
2. Add std-only public-register probes in `crates/nec-upd765a/tests/read_track_fields.rs` and this directory. Reuse the drained probe host and existing fixture-only MFI encoder/decoder. No dependency, production interface or snapshot change.
3. Capture twice: N0–5, N0 DTL boundaries and nonzero contrast, commanded/recorded size differences, short/equal/long/empty representations, missing/empty tracks, CHRN mismatch, duplicate and reversed IDs, EOT boundaries. N6/N7 and invalid N are input-safety controls only. Bound emulated ticks, payload and host time. Record every payload/result byte, idle/recovery and immutable media.
4. Independently audit raw fields/CRCs/clocks, input and tool hashes, repeats and evidence completeness. Deliberately damage expectations, observations and raw fixtures to prove rejection. Preserve disagreements and unsupported representations explicitly.
5. Write a readable report and durable evidence, run controller tests/format/Clippy as appropriate, push a reviewable owning-repository PR, and update #1870. Keep all operations partial. Update umbrella progress separately when supported by evidence.

Verification: fresh observations must be nonempty and repeated; no process-exit-only pass. Installed MAME source identity remains unknown and separate from inspected source. Fuse source must be clean. Firmware remains local, temporary copies removed. The historical donor directories named by repository guidance are absent; reuse the current committed probes instead. Do not edit vendored references or unrelated worktrees.

Cleanup: task workspace `output/qualification/workspaces/read-track-field-boundaries`; retain exact source-shaped export and raw logs outside the workspace before removal. Remove only task-owned builds/processes, after checking remote retention and untracked/ignored files.
