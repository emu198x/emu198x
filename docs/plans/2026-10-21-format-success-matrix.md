# Format success component matrix

Spectrum launch-hardening, umbrella237. Component register access only; no
ROM loading, reference execution, native restoration or operation-wide verdict.
No runtime/API/schema changes. Reuse the existing public controller methods.

Freeze twelve fitting sector-model cases before adding tests, each run in
ND0 and ND1: MFM N0,1,2,3,4,5; FM N0,1,2,3,4; standard MFM N2/SC9.
Single-sector cases use GPL42, filler69 and authored ID [7,8,255,3], which
separates the physical selector, command size, ID size and wrapping result R.
The standard case uses GPL82, fillerE5 and ordered IDs [9,1,8,2,7,3,6,4,5].

Expected invariants come from NEC December1978 printed14 Format: command N
sets payload size, supplied CHRN records IDs in order, D fills every payload,
result R is incremented. Sector-image replacement clears old error/deleted
metadata and removes old trailing sectors, an explicit digital-model promise
already covered by existing unit tests. Do not infer physical old-track gaps.
Check each request/result direction, IRQ, exact seven-byte result, no active
export, untouched other physical track, and recovery to idle. Bound every wait.

Verify all24 fresh cases twice. Deliberately change expected R in a disposable
copy and prove the exact test fails, then restore it and rerun. Run fmt, owning
all-target Clippy and chip suite. Record nonzero counts and raw logs; preserve
active READY/Fault and the rest of237 as unqualified work. Push normal PR.
