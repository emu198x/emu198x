# Original Acid800 ANTIC probes

`acid800_antic.rs` runs all twenty original ANTIC standalone probes in PAL and
NTSC through the same hash-pinned runner as [GTIA](acid800-gtia.md). Avery Lee's
guest executables remain unmodified. No ROMs, XEX files, symbol files or source
material are distributed with the harness.

The manifest pins each XEX and MADS symbol file plus the XL OS/BASIC firmware.
Every guest starts from the same fresh BASIC READY snapshot and must return to
`_testEnd` with the expected Y status and diagnostic. Timeouts, skips and unknown
statuses cannot match the recorded pass/fail baseline. Failures are reported as
hardware failures even when their signatures match; a green baseline test is
not an ANTIC conformance claim. See the GTIA README for loader limits and setup.

```sh
EMU198X_ACID800_ROOT=/path/to/Acid800/standalone \
EMU198X_ROMS_ROOT=/path/to/roms \
EMU198X_ACID800_REPORT_DIR=/tmp/acid800-results \
cargo test -p machine-atari-800xl --test acid800_antic -- --ignored --nocapture
```

Reports are `antic-ntsc.json` and `antic-pal.json`, separate from the GTIA
reports. `EMU198X_ACID800_STRICT=1` requires every guest to pass and currently
fails. The normal mode checks exact known outcomes without hiding failures.

## Recorded baseline

| Probe | NTSC | PAL |
|---|---|---|
| addresswrap | pass | pass |
| addrmirror | pass | pass |
| blockednmi | pass | pass |
| charcontrol | pass | pass |
| default | pass | pass |
| dlistwrap | pass | pass |
| dlitiming | pass | pass |
| dmapattern | pass | pass |
| hiresbug | fail | fail |
| hscrolbug | fail | fail |
| linebuffering | fail | fail |
| nmist | pass | pass |
| pfstarttiming | fail | fail |
| pfstoptiming | fail | fail |
| pmdma | pass | pass |
| vcount | pass | pass |
| virtdma | fail | fail |
| vscroldli | pass | pass |
| vscroll | pass | pass |
| wsync | pass | pass |

**28 passes and 12 failures across 40 executions.** The initial survey had
10 passes and 30 failures. Correcting the playfield counter's 4 KB wrap makes
the full address-wrap guest pass in both regions. Cycle-7 NMIST latching,
cycle-8/9 NMI assertion and NMI edge capture during RDY holds also make
the NMIST and complete DLI-timing probes pass. The counter wraps for character-name and bitmap fetches and when
advancing between mode lines. LMS continues to select the upper four bits.

A probe name does not isolate the failing chip. Correcting POKEY RANDOM polarity, long-polynomial
feedback and restart phase makes the complete WSYNC and DMA-pattern probes
pass in both regions. Blocked-NMI tests exercise CPU interrupt sequencing as well as ANTIC.
The first failing assertion can hide later failures in the same executable.

Source trail: Mapping the Atari's screen-RAM/LMS restrictions; Altirra's
separate playfield page and masked 12-bit offset; the original
`antic_addresswrap.s` collision-based check. Detailed investigations and guest
reports are retained in the private shared reference library.

Retaining the display instruction when list DMA is disabled and preserving
scroll-region state across vertical blank make the complete display-list-wrap
and vertical-scroll guests pass. Separate cycle-6 DLI and cycle-109 row-stop comparisons also make the
VSCROL/DLI timing guest pass. Blank instructions can end scrolling regions.

A two-clock ANTIC NMI pulse and the CPU detector's vector-window inhibit
resolve the blocked-NMI guest. Longer NMI assertions remain pending, preserving
the NES and C64 interrupt regression cases.
