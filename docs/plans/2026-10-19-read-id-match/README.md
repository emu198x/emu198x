# Read Data now compares the complete requested sector ID

[Issue #1841](https://github.com/emu198x/emu198x/issues/1841) prevents Read Data
and Read Deleted Data from delivering a field with the wrong H or N. The
controller searched its selected physical side using only C and R. That could
truncate/pad a wrong-size field into a read or hide a later exact duplicate
behind an earlier mismatching H/N record.

The fix adds H and N to that predicate. Physical side selection remains driven
by HD; the recorded H byte is matched against command H. If no exact record
exists, the existing ND/cylinder-error path and result draining apply. Read
Track, result CHRN, byte clocks, MT and DTL are unchanged. No dependency, API or
serialized-field change is needed.

Four public-register tests fail against source
`1a17cf4779f397c44de03312bacefd39cfa9083a` (exit 101, zero passed, four failed).
The retained raw log shows wrong-H data delivered, wrong-N data delivered,
the earlier wrong duplicate payload returned, and a read satisfied on a side
whose recorded H did not match. A failed test stops at its first assertion;
these are four concrete failures, not a baseline run of every later branch.

After the predicate change the same behavior checks pass, including both data
marks, matching recovery after an ND result, later exact duplicate selection,
physical side versus ID H, all seven result bytes, idle recovery, and unchanged
source metadata/payload. The four tests execute 40 successful bounded commands.
H1 on physical side 0 is an explicitly requested recorded ID. H7/FF fixtures
also verify full-byte matching in the represented image model; they do not
qualify nonstandard hardware head configurations. Result H still follows the
existing physical-head convention and remains separate work.

All 127 controller tests pass. Clippy with warnings denied and rustfmt pass.
The initial Clippy run rejected a test-only seven-element signature tuple as
`type_complexity`; the signature now groups metadata in a byte array. The
initial failure log is retained alongside the successful check.

The held NEC December 1978 sheet defines ID H and selected HD separately on
printed p9 and describes Read Data ID search/ND on p12. Its prose abbreviates
matching as R, so it is not cited as explicit proof of the full predicate on
its own. Stock Fuse seek_id and MAME sector_matches compare C/H/R/N; exact
source revisions and hashes are in evidence/result.json.

Fresh stock-Fuse source-controller execution reuses the existing raw-track
adapter with the unmodified controller/drive/CRC implementation. All 36 bounded
runs (18 repeat pairs) agree: wrong H/N deliver no payload; H, N and combined
mismatching duplicates do not hide a later exact record; an explicitly requested
recorded H1 on physical side 0 reads; a requested N1 field returns 256 bytes.
Corrupting the actual data CRC produces DE/DD after the payload. The offline
auditor independently checks authored raw header/data CRCs, recorded CHRN,
bytes delivered, complete results, idle state, media immutability, source/tool
identities and sanitizer-clean logs. Each run has a two-second emulated bound
and a ten-second host timeout. This is not the full Fuse application or ROM
loading. MAME was inspected, not freshly executed here.

Reference disagreements stay visible: failed H/N searches yield Fuse ST1=05
(MA|ND), while the existing represented-ID Emu path remains 04 (ND). Fuse also
returns command H while current Emu returns physical head in result H. Neither
is changed here. No physical search timing, result-CHRN conformance, Read Track,
MT, DTL, raw-gap or flux claim follows from the predicate regression.

The parent validated clean source 8db4b82a38844bcbd45df28cfd3626190910ebee:
337 runtime/+3 tests pass, 63 are ignored without credit, and the normal locked
headless release build passes. Together with the controller suite that is
464 passing tests. Two cold boots select the normal +3 ROM Loader and read the
captured diagnostic DSK through the FDC. Both record 519 native checks, eight
firmware checks, three loading checks and seven execution checks. The program
changes the initially clear completion bytes to 5A5B, records writes at PCs
8030/8034, and fills the expected AA bitmap/47 attributes. Sampled PC is 8036;
the two instruction PCs demonstrate completion rather than guessing from one
sample. Screen-RAM SHA256 is
`d2885ec2c4cc1df7ac1a7eecac91ccdd3a6166563d09c2142ec594dc3bfa92f2`.
The incorrect expected signature 5A5C fails the execution check while loading
passes. Wrapper exits are 0/0/1; all runtime processes exit 0, proving transport
success does not qualify the negative case.

These are source-build runtime regressions for the named PAL +3/40077/128 KiB,
English v4.0 four-ROM configuration and standard 180 KiB drive-A disk. Assembly
and mastering are captured fixture replay, so all records remain
qualified=false. No firmware, executable or native machine state is committed.
The two safe archives retain exact commands, environment, source/executable/
fixture/firmware hashes, scripts, raw observations, relevant logs and screens.
Each file was compared byte-for-byte with its original before archival.

Run `python3.14 docs/plans/2026-10-19-read-id-match/verify.py` to audit the
retained evidence and eight failing mutation controls: corrupt evidence,
missing artifact/test, zero assertions, wrong reference payload, false execution
pass, empty tests and wrong exit. Source, lock, compiler and final focused-test
executable identities are recorded. The red executable hash was not captured;
the base source and exact red test source are retained instead. No distributed
release or broader Spectrum/FDC qualification is claimed.
