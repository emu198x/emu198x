# MOS 6581 / 8580 SID

Sound Interface Device — the C64's audio chip. Three voices, each with a 24-bit phase-accumulator oscillator, four selectable waveforms (triangle, sawtooth, pulse, noise), hard sync, ring modulation, and an ADSR envelope. All three voices feed a shared state-variable multi-mode filter (LP/BP/HP) with routing selectable per voice. Master volume, voice-3 mute, paddle ADC readback, and oscillator/envelope introspection registers round out the register map.

## Crate

`mos-sid-6581` — **ported.** Second C64 chip after `mos-cia-6526`. Source provenance in the [C64 per-subsystem source map](../decisions/archives-as-source.md#c64). 9 unit tests cover silence, sawtooth bipolar swing, ADSR attack→sustain, ADSR release decay, OSC3 oscillator introspection, ENV3 envelope introspection, state-variable filter low-pass attenuation, main-buffer draining, and the per-voice/main buffer length invariant.

## Architecture

Four source files, matching the archive's decomposition:

- `envelope.rs` — ADSR state machine. Rate counter + exponential counter, reSID die-analysis period lookup at six level thresholds (≥0x5D → 1, ≥0x36 → 2, ≥0x1A → 4, ≥0x0E → 8, ≥0x06 → 16, else 30) to approximate the 6581's analog decay curve.
- `voice.rs` — 24-bit phase accumulator, four waveform generators, 23-bit LFSR noise source, hard-sync and ring-mod inputs from a paired voice, combined-waveform lookup tables (6581) or AND-mix (8580).
- `filter.rs` — Two-integrator state-variable filter. 6581 cutoff curve is a 32-point piecewise-linear table from reSID die analysis (captures the low-end 200 Hz floor, the steep midrange kink, and the gradual high-end ramp). 8580 curve is linear. Resonance ceiling differs between models.
- `lib.rs` — `Sid6581` struct wiring all of the above together, register bus (`read(&self, addr)` / `write(&mut self, addr, val)`), per-tick pipeline, and downsampling from φ2 rate to host sample rate.

## Pin contract

The SID has no cross-chip bus visibility. Its pin contract is minimal:

**Input pins (machine → SID):**
- `potx: u8`, `poty: u8` — paddle ADC readings (0..=255, centre 0x80). The machine samples paddles and writes these; the CPU reads them back through `$D419` / `$D41A`.

**Output (not pins, but data stream):**
- `take_buffer() -> Vec<f32>` — drains the mixed audio output as `f32` samples normalised to `-1.0..=1.0` at the host sample rate.
- `take_channel_buffers() -> [Vec<f32>; 3]` — same but per-voice, for channel-isolated capture and MCP introspection.

No IRQ. No BA / RDY. No shared bus reads. The register bus at `$D400-$D41C` is method access (same rationale as `mos-cia-6526` — only the CPU talks to it, no cross-chip observation to preserve via pin fields).

## Tick pipeline

The machine calls `sid.tick()` once per φ2 cycle. Each tick:

1. Capture previous accumulator MSBs (for hard-sync edge detection).
2. Step all three phase accumulators by their frequency registers.
3. Clock the three noise LFSRs on their bit-19 rising edges.
4. Apply hard sync (voice 0 ← 2, 1 ← 0, 2 ← 1).
5. Clock the three envelopes from their gate bits.
6. Compute each voice's waveform output with ring modulation (voice 2 → 0, 0 → 1, 1 → 2) and scale by the envelope level.
7. Route voices through the filter or directly to the mixer per `$D417` bits 0-2.
8. Mix through the SVF, apply master volume, normalise to `-1.0..=1.0`.
9. Accumulate into the downsampling window; when `sample_count ≥ cpu_freq / output_rate`, emit one sample to the output buffer and reset the window.

## Deviations from the archive

- **Serde everywhere** — full `Serialize`/`Deserialize` derives on `Sid6581`, `Voice`, `Envelope`, `Filter`, `Phase`, and `SidModel`. The two audio `Vec<f32>` output buffers carry `#[serde(skip)]` so save states don't capture pending audio.
- **`as i16` instead of `u16::cast_signed()`** — the archive uses the new stable method introduced in Rust 1.87; our workspace's MSRV is 1.85.

## Known gaps (deliberate)

- **External filter input** (bit 3 of `$D417`). The register bit is captured but the audio path doesn't exist — the C64 doesn't route anything to the SID's EXT IN pin on a stock machine.
- **Paddle ADC timing**. Real hardware takes ~512 φ2 cycles to complete a paddle conversion. This model treats `potx` / `poty` as plain fields the machine updates; timing is not simulated.
- **6581 "DC bias" distortion**. The 6581's analog output path had a DC bias that made voice-3-mute sound distinctive (the "SID sample player" trick). Not modelled.
- **reSID-grade analog accuracy**. The filter curve is piecewise-linear from die analysis, but the resonance Q, clipping, distortion, and voltage-follower non-linearities are approximated. For cycle-accurate reproduction of recorded SID tunes with analog character intact, the long-term path is a reSID wrapper (flagged as an open question in [product-roadmap.md](../decisions/product-roadmap.md)). For now, this port is sufficient for tutorial-level sound output.

## Related

- [Archives as source](../decisions/archives-as-source.md) — per-subsystem port-source decisions.
- [CPU bus interface](../decisions/cpu-bus-interface.md) — the pin-level contract (and its non-applicability to peripheral register buses).
- [Product roadmap](../decisions/product-roadmap.md) — open question about the long-term SID approach (port / rewrite / reSID wrapper).
- [MOS 6526 CIA](mos-cia-6526.md) — sibling chip, same "register bus is methods, everything cross-chip is fields" pattern.
