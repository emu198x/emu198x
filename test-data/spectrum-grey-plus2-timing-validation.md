# Grey +2 interrupt phase

The grey +2 now selects its own interrupt profile at construction and after
state restoration: assert at ULA pixel 3 and release at 75, a 36-T-state pulse one T-state
before the early Toastrack's assertion at 5 and release at 77. Raster fetches, contention,
frame length and the CPU interrupt-sampling deadline are unchanged.

The variant marker reattaches the appropriate ULA configuration. No field is
added to serialized state, so existing state layouts remain compatible.
ROM-free regressions check assertion and release over two frames, including
restoration before, during and after a pulse, followed by reset. The new +2
regression fails on the original core; the Toastrack regression passes there.

## Evidence

[Published physical +2 reports](https://github.com/redcode/ZXSpectrum/wiki/ZX-Spectrum-Timing-Tests-128K#results-on-real-hardware)
record no failures in the Butler suite. The original core instead produces
the five early-Toastrack failures, using the actual +2 type and its own ROMs.
The source is pinned at wiki revision
`4b9e47af926cbb46972db5cdc99badaa2fb3d771`.

SpecIde `56bee623f18749d0d261d49c7dbdc2654d850bca`, `source/src/ULA.cc`,
selects separate `ULA_128K` and `ULA_PLUS2` interrupt starts. A two-frame
signal trace gives +2 assertion/release at native ULA edges 113091/113163
and 254907/254979. The new +2 regression pins those values. SpecIde's
Toastrack pulse is 35T; our independently validated Toastrack remains 36T.

Primary family evidence and original replay adapters are retained in
`198x/reference/by-system/sinclair-zx-spectrum/zx-spectrum-grey-plus2-timing-evidence.md`
and `198x/ops/experiments/spectrum-grey-plus2-profile/`.
These are published hardware reports plus implementation precedent, not a
new physical-chip capture. They do not establish every board revision's timing.

## Reproduction and scope

Both models use the same survey implementation and immutable SZX, with
separate ROM variables, machine types, expectations and JSON output paths.
The raw suite verdicts remain visible; +2 requires all-pass while Toastrack
requires its five exact early-profile readings and passes elsewhere.
Reports include both ROM hashes.

```sh
cargo test --release -p runtime-sinclair-zx-spectrum \
  --test timing_survey_128k timing_survey_plus2_records_every_case \
  -- --ignored --nocapture
```

Set `EMU198X_SPECTRUM_PLUS2_ROM0`, `EMU198X_SPECTRUM_PLUS2_ROM1` and
`EMU198X_ZX_SPECTRUM_TESTS_DIR`. The nightly corpus currently provisions only
Toastrack ROMs, so it explicitly excludes this +2 fixture test; ROM-free +2
pulse and restore tests run in ordinary CI.

HALT2INT128 is also used as an observational diagnostic: its classification
changes from Early/Early to Late/Late. Local Floatspy/HALT2INT reference images
are labelled 128K and are not promoted to grey +2 hardware oracles. A full +2
Floatspy self-test and board-attributed +2 tape-probe targets remain outside
this validation.
