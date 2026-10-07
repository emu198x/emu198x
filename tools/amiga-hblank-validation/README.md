# Requalifying the static HBLANK references

`requalify.py` verifies the two registered package identities and all selected
record, capture, manifest, log and decoded-pixel hashes. It requires 28 runs
and three byte-identical fields per run, then remeasures the black intervals.
It does not read native images.

The packages agree on nine observations in their declared HB-register
coordinates and disagree on five. Copperline applies a post-render mask;
those comparator agreements do not establish absolute signal timing.
Absolute native phase uses the independently traced UAE origin, native=raw+6
host-hires samples. Native lores [4,381) contains every shared sample except
UAE's two-sample storage prefix. The verifier keeps both claims separate.

Run the full consumer with `scripts/verify-amiga-programmable-hblank.sh`.
The wrapper builds the corpus, verifies references, then passes the generated
observations to the Rust consumer. Run verifier checks with:

```sh
python3.13 -m unittest discover -s tools/amiga-hblank-validation
```

Primary evidence and counter origins are recorded in
`reference/by-system/commodore-amiga/2026-ecs-colour-blanking-observations.md`
at the family root. The retained input packages are unchanged.
