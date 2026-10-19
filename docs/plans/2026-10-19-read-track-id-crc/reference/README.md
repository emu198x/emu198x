# Read Track ID-CRC error before payload completion

This capture supports issue emu198x/emu198x#1849 through unmodified Fuse's
controller, drive and CRC source. It is a source-controller experiment, not a
full Fuse application, ROM-loading run, silicon timing qualification or a
qualification verdict. All records retain qualified=false.

The one-sector raw fixture is C0/H0/R1/N2, normal MFM data mark, 512 payload
bytes of 21. Command 42 selects physical HD0, requests the same C/H/R/N, and
EOT1. The authored host changes actual CRC bytes, not controller status:
mask1 flips one bit in the ID CRC; mask2 does the same in the data CRC. Mask0
is clean; mask3 corrupts both. Header/data bytes and their missing-clock marks
otherwise remain byte-identical. The independent auditor validates both CRCs
and all six A1 missing-clock positions and checks that only the requested CRC
bytes differ from the clean track.

Each corruption type runs in two fresh processes after consuming 0, 1, 17 or
512 bytes: 32 observations, sixteen identical repeat pairs. At 0/1/17 bytes the
host stops servicing requests and lets stock Fuse's coarse timeout fire; 512
bytes complete normally. All seven result bytes drain, MSR returns to idle,
and the original raw medium remains unchanged.

| CRC corruption | Interrupted result ST0/ST1/ST2/C/H/R/N | Completed result |
|---|---|---|
| none | 40 10 00 00 00 01 02 | 00 00 00 00 00 02 02 |
| ID only | 40 34 00 00 00 01 02 | 00 24 00 00 00 02 02 |
| data only | 40 10 00 00 00 01 02 | 00 20 20 00 00 02 02 |
| both | 40 34 00 00 00 01 02 | 00 24 20 00 00 02 02 |

The ID-only interior interruption retains DE while DD stays clear. Corrupting
only the data CRC sets neither bit before that CRC is reached. This is the
bounded observation relevant to the proposed Emu correction. Do not transplant
the full status/result values: Fuse also sets ND for an ID CRC error and has
separate completed-command ST0/EN/result-R behavior. The existing MAME/Emu
versus Fuse completed-data-CRC/result-R disagreement is not settled here.
The both-corrupt raw case is an additional independent control; represented
sector metadata DE/DD alone cannot distinguish it from data-only corruption.

The adapter reuses the previously retained event host and raw-track probe.
Only the authored fixture/command/drain logic changes. It compiles stock
upd_fdc.c, fdd.c and crc.c from Fuse commit
0ff7fe676394aa357f49b6fbdc3ecb38a6592ab2, controller SHA256
845b960eb68cba9b21cba00cdc3c03cf0d4dc491ffd96efecb1d53336ce7f72d.
Apple Clang 21.0.0 builds with ASan/UBSan; all32 runtime stderr logs are empty.
Executable SHA256:
fa2c0d3e956fd6261c665daaa7ac6823b64334a40a1176d9c0bf6ea5e070244b.
Exact argv, environment, source/compiler/executable hashes and limits are in
runs/campaign.json. Every execution has a15-second host timeout and a2-second
emulated bound. The drive is an authored ready raw-track fixture, so motor
spin-up and disk-container decoding are outside this experiment. The host
clock remains the existing3546900-T-state setting; its400ms timeout does not
qualify Emu's byte deadline or any A/B silicon-specific exception.

To inspect the captured evidence:

```sh
python3.14 verify.py
python3.14 -m unittest -v test_verify.py
```

The eight tests include rejection of lost ID DE, prematurely published data
DE/DD, an uncorrupted CRC control even with a recomputed artifact hash, missing
repetition, patched-reference identity and false qualification. They execute
no emulator. To capture again, copy the authored sources to a new directory
with no runs directory and execute python3.14 run.py. It requires the recorded
local stock Fuse path and Clang, not firmware. Compiled binaries/debug bundles
and caches are excluded from the archive. No proprietary firmware or native
machine state is included. The parent owns the Emu fix, release and ROM checks;
no upstream report was sent.
