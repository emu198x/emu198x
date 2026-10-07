# Nominal Paula sample amplitude

The inherited `x - 0.02*x*x*x` curve changes every nonzero sample. Its original
comment attributes it to A500/WinUAE measurements without identifying a
capture or transfer curve. The correction removes that unsupported effect;
it does not claim that physical DACs have no imperfections.

The runner reuses pinned handover extraction, then executes unchanged
WinUAE sample/startup methods and `DO_CHANNEL_1`, and vAmiga startup and
`penhi` methods. All four channels and 256 byte encodings run at volume 64
and 127. Both settings force full volume, so PWM phase cannot affect output.
Both references agree on all 2,048 rows. The checker rejects empty data and
one corrupted amplitude. The compressed C++ retains upstream copyright and
licence notices and is not linked into Emu198x.

CSV columns: `channel,volume,encoded_byte,scaled_signed_sample`.
The scaled sample is signed byte × 64. Native mixer units divide by 16,384
to include its stereo half gain. The native regression consumes the compiled
WinUAE CSV and checks amplitude and routing; before correction 2,040 cases
failed, with only eight zero-sample cases agreeing.

```sh
python3.13 test-data/commodore/amiga/paula-audio/amplitude-probe/reference.py \
  --winuae ../../emulators/amiga/WinUAE --vamiga ../../emulators/amiga/vAmiga \
  --output /tmp/paula-amplitude
cargo test --locked --release -p emu198x-commodore-paula-8364 --test audio forced_full_volume_preserves
```

The primary evidence is
`reference/by-system/commodore-amiga/2026-paula-sample-amplitude-observations.md`
in the umbrella repository. Real DAC calibration and PWM timing remain open;
this correction changes neither timing nor snapshot v61.

After correction, all 2,048 amplitudes match. Validation passes 120 component
and 128 affected runtime tests, strict Clippy, release build and all three
ROM-backed waveform cases. All five reference artifacts regenerate exactly.
`validation.json` and compressed logs retain the failing baseline and final
gates, including an initial command error from nonexistent test target names;
the corrected run includes `snapshot_roundtrip` and both actual Paula board
tests.
