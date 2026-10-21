# Run the unchanged zxsp controller for all-skipped research

Lane: Spectrum launch-hardening. Owner: Emu #1949; umbrella #231.

1. Inspect current source, prior raw-capture studies and binding evidence rules.
2. Compile byte-identical Fdc765.cpp/.h from pinned zxsp against a small host/drive
   scaffold. No edits to vendored code, no new dependencies, no ROM or CPU bypass
   claimed as loading qualification. Record exactly which components are replaced.
3. Reuse the existing authored raw-track recipe. Run both read opcodes, SK on/off,
   matching/mismatching marks, single/two-sector tracks, twice in fresh processes.
   Bound each emulated command and host process. Retain raw register observations,
   payload, status, recovery, input/live media, commands and executable identities.
4. Audit raw observations rather than summaries; deliberately reject altered raw
   data and source identities. A failed SK-off control limits oracle applicability.
5. Record the actual outcome and unresolved silicon identity. Do not change Emu's
   expected status solely to match an emulator. Push a reviewed research PR,
   update issues and remove disposable build/worktree after verifying retention.
