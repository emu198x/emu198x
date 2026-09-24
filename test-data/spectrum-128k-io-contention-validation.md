# Sinclair 128K I/O contention validation

The 128K/+2 wrapper now uses the port lookup rules already established on
48K. FUSE 1.7.0 `machines/spec128.c` selects the same ULA decode and contention
functions; `peripherals/ula.c` defines the early/late lookups. The 128K's page
classification remains dynamic: fixed bank 5 and odd paged RAM are contended.

The former level gate produced 984 mismatches in 10,596 short-run observations.
The new ROM-free regression covers IN A,(C), OUT (C),A, six port addresses,
banks 0/1/2/3, and eight starting skews. The corrected implementation agrees
in 10,884 short-run and 1,894,368 full-frame observations. Different totals
reflect changed instruction costs inside a fixed elapsed-time window.

The oracle's 14360 pattern-coordinate offset is inherited from the physical
counter reconciliation; it is not fitted to this change. Expected costs use
independent page classification and FUSE's early/late sequence, while the
machine uses public CPU pins and stalls its clock. No new state or timing
constant is introduced in production.

Float128K remains 14364. Full Floatspy, its initial read, HALT2INT, btime and
ptime match the existing hardware-derived screen oracles. No target or golden
changes. Ordinary affected ULA, core, 128K and +2 tests and all-target Clippy
pass. The eihalt fixture is unavailable and was not run.

Run the ROM-free full-frame check:

```sh
cargo test --release -p machine-sinclair-zx-spectrum-128k \
  --test io_contention_oracle -- --include-ignored --nocapture
```

This is selected reference-emulator and hardware-derived software evidence,
not a physical-chip capture or proof for all bus histories. The grey +2 shares
the wrapper but has no separate hardware capture here. Timex and Amstrad gates
remain outside this correction.
