# Format Track must honour initial independent READY

Spectrum launch-hardening; umbrella237 and Emu1962.

## Contract and cause

`start_format_track` tested mounted media and protection without sampling the
existing `drive_ready` input. NEC December1978 printed p14 Format specifies
abnormal termination when READY is initially absent. Vendored MAME
`src/devices/machine/upd765.cpp`, `format_track_start`, checks READY before
write protection and returns ST0 NR|FAIL. The bounded fix applies that check
before execution or allocation, using existing serialized fields.

## Work and verification

- Public-register fail-first tests observed MSR30 instead of resultD0 with a
  writable image still mounted. Preserve the original log separately.
- Check initial READY=false for FM/MFM, ND0/ND1 and protected/writable media;
  assert exact seven-byte result, IRQ acknowledgement and unchanged media.
- Assert a fresh READY=true command actually formats exact requested bytes
  after rejection; service waits are bounded.
- Check native restore before command, at partial command, and at all eight
  result positions, retaining initial READY, status, IRQ and unchanged media.
- Run chip/runtime suites, fmt and clippy; commit/push normal review workflow.

## Explicit unresolved observation

The initial reproducer also dropped READY during Format preamble and observed
continued execution. The generic NEC A/B ST0 READY-change table alone does not
establish when Format samples an active change, and MAME samples at start;
its READY polling is idle-only. No immediate C8 result is adjudicated here.
The speculative active C8 test is replaced by this record, not counted as a
pass or an ignored test. Active-loss semantics remain separate research and
remain visible in Emu1962 and umbrella237. This fix does not qualify Format
Track as a whole, physical READY timing, head mechanics, flux, or raw gaps.

No dependency, public API or snapshot schema change.
