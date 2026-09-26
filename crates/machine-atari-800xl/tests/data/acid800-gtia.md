# Original Acid800 GTIA probes

`acid800_gtia.rs` runs Avery Lee's eleven standalone GTIA tests on an Atari
800XL in NTSC and PAL configurations. The guest executables are unmodified.
The manifest pins SHA-256 hashes of the XEX files, matching MADS symbol files,
and Atari XL OS/BASIC firmware. No firmware or test binaries are distributed
with this harness.

The local reference corpus is Acid800's `out/Release/Acid800/standalone`
directory; its source is `src/Acid800/source/gtia_*.s` and `library.s`.
`gtia_psuedomodee` retains the upstream spelling. The `.lab` files identify
`main` and `_testEnd`; `library.s` returns Y=$00 for pass, $80 for failure,
and $40 for skip before `_testEnd` waits for input. Hashes, rather than a
source version guess, identify the exact build exercised here.

Run from the emulator repository, using legally obtained local firmware:

```sh
EMU198X_ACID800_ROOT=/path/to/Acid800/standalone \
EMU198X_ROMS_ROOT=/path/to/roms \
EMU198X_ACID800_REPORT_DIR=/tmp/acid800-results \
cargo test -p machine-atari-800xl --test acid800_gtia -- --ignored --nocapture
```

The firmware directory must contain `atari-800xl/atarixl.rom` and
`atari-800xl/ataribas.rom` matching the manifest. Missing environment settings
use the repository's explicit fixture-skip mechanism; configured missing or
changed files fail. Tests are ignored by default because these inputs are local.

Each region boots for 700 frames and must reach BASIC READY without a halted
CPU. Every probe starts from a fresh copy of that boot state. The existing XEX
parser loads segments and RUNAD; this deliberately limited loader rejects
INITAD segments. A probe has 30 million colour clocks to return. Timeout,
skip, unknown exit status, and changed failure text cannot match a known
failure. Screen text is retained in the optional JSON reports; whitespace is
ignored when comparing wrapped diagnostic signatures.

## Recorded baseline

| Probe | NTSC | PAL |
|---|---|---|
| default | pass | pass |
| consol | pass | pass |
| addrmirror | pass | pass |
| collision | pass | fail: missing left-edge M/P collision at $22 |
| collision2 | pass | pass |
| phantomdma | fail | fail |
| pmoverlap | fail | fail |
| pmresize | fail | fail |
| pmretrigger | fail | fail |
| psuedomodee | fail | fail |
| vdelay | fail | fail |

There are **9 guest passes and 13 guest failures**, across 22 executions.
A passing baseline test means those exact observations were reproduced;
it does not mean GTIA conformance. Set `EMU198X_ACID800_STRICT=1` to require
all guests to pass instead. Strict mode still writes the reports before
failing. A newly passing probe intentionally fails the baseline comparison
until its expectation is reviewed and updated alongside the accuracy fix.

These tests expose interacting CPU/ANTIC/GTIA behaviour. A guest failure
identifies a reproducible symptom, not necessarily the chip responsible.
The corpus stops each probe at its first failing assertion, so resolving a
failure may reveal further failures within the same probe. PAL's extra
collision failure is recorded separately; it must not be hidden by NTSC's
passing result. This harness does not change emulator timing.
