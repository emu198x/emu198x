# Selected media changes terminate active reads

[Issue #1836](https://github.com/emu198x/emu198x/issues/1836) prevents a command
from combining cached bytes from one disk with sectors from another disk.
This is the existing host media-change abort policy extended to reads; it
asserts no exact physical READY transition timing or interrupted hardware CHRN.

The public-register probe ran Read Data, Read Deleted Data and Read Track at
three boundaries: before the first request, after one consumed byte, and
between sectors after 512 bytes. Each of replacement, ejection and an unrelated
drive change ran twice against both source revisions (108 observations). A
further 54-observation streamed-source replay was byte-identical.

| Selected drive 0 change | Whole-command source c5586b620 | Streamed source a62d05994 |
|---|---|---|
| Replace before first request / within R1 | Old R1 + old R2 | Old R1 + replacement R2 |
| Eject before first request / within R1 | Both cached old fields | Cached old R1, then error |
| Replace/eject after consuming R1 | Both cached old fields | Both cached old fields |
| Change unrelated drive 1 | Original read unaffected | Original read unaffected |

The streamed read prepares the next field after draining the current field.
`insert_disk_with_writable` and `eject_disk` previously aborted only writes,
formatting and Scan; the still-active read retained its cached current field
and later selected a field from the replacement image. Existing whole-command
buffering already hid ejection from active reads, but did not mix images.

The fix extends that selected-drive guard to the three read commands. It
reports the established NR/abnormal policy and calls read_result, retaining
encountered status/current R while clearing pending read progress and timing.
No further payload requests arrive, results drain normally, and a fresh
command reads the replacement. An unrelated drive change remains inert.
No dependency, API, serialized field or snapshot version changes here.

Four regressions cover 30 selected-media-change combinations (including first
and second request high/low boundaries), 18 unrelated-drive combinations,
completed Read Track CRC retention, and encountered CM without premature CRC.
Before the fix the first two relevant tests fail while the unrelated-drive
control passes (one passed, two failed, exit 101). After the fix all four pass,
as do all 121 controller tests, Clippy with warnings denied, and rustfmt.

Reference source was inspected, not executed in this capture. Fuse
0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2 fdd_unload clears ready/loaded;
fdd_read_write_data does not access track data when not ready, and ID search
stops when readiness disappears. MAME 9391e1eff80eb04abd58d48218a1d3b9643c1ea8
floppy call_unload removes its image and marks not-ready. Its controller's
READY polling is gated to command phase. These establish different reference
mechanisms, not one exact immediate result/status rule for an in-flight read.

`observations.tar.gz` contains 19 safe regular files: authored probe, standalone
Cargo manifests/lock, source/tool/executable identities, observations, red/green
logs, checker and compile logs. It contains no firmware, executable, native
state or vendored source. Exact source and artifact hashes are in summary.json
and the archive's provenance.json/hashes.json. The standalone probe resolved
cached offline dependencies; the production tests use the workspace lock.
Every record retains qualified=false; this is controller bug evidence, not a
ROM-loading qualification.

Extract to a new directory and run `python3.14 verify.py`. The checker verifies
all 108 comparison observations and 54 replay observations, verifies hashes,
and rejects invented old-media-only payload, missing result and zero-length
controls. Source worktree paths in build manifests are provenance; re-create
those exact revisions in isolated checkouts to rebuild. No original machine
state is required.
