# Published Windows and Linux developer handoffs pass

Native run [38937404950](https://github.com/emu198x/emu198x/actions/runs/38937404950)
passes on Windows x64 and Linux x64 using public Emu 0.35.2, Asm 0.0.67 and
Build 0.2.12. The reviewed release source is
42e28e284d2b3120e850aea66bfcf097b6119afa; actual tag producer
[38933615684](https://github.com/emu198x/emu198x/actions/runs/38933615684) succeeds.
Both public-release verification records check downloaded archive/executable
bytes and receipts against that producer's native report and reject four changed
receipts each.

Each host executes all 13 commands: the maintained Meteor Storm recipe makes
six TAP blocks, loads through original firmware and reaches the expected program
PC, title RAM and rendered glyphs. The real missing-CODE medium fails the same
runtime verifier. Each host also assembles and ROM-loads the original first
lesson, loads its emitted debug sidecar and stops at source line 9 / $8004.
A real missing-sidecar request exits 1; a wrong source location is rejected.
Windows retains its 1 MiB PE stack reserve. Firmware cleanup and safe evidence
upload succeed on both hosts. No firmware or release executable is retained.

The umbrella completion verifier compares Windows/Linux canonical products and
matches both source-debug products to the earlier macOS observation. These are
scripted archive-based developer journeys; GUI and vendor-installer usability
remain unqualified. Original failed runs and their original hash manifest remain
unchanged. `published-sha256.json` hashes this final evidence separately.
