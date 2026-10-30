# Save the complete Spectrum video pipeline

Adopted 2026-10-30 for #2209. Steve chose fixing save fidelity before the Timex board split and explicitly approved version 17 with rejection of version 16.

This supersedes the ULA transient-default justification in the serialization audit introduced by Seam 3 of `spectrum-architecture-review.md`: draining within a character cell is insufficient when the first resumed pixels change. The audit still locks every allowed skip; only the immutable engine timing reference may remain omitted.

The shared ULA serializes its pending bitmap and attribute fetches, second Timex bitmap fetch, border output latch, VidEN, M-cycle falling phase and previous falling address. These are live state even when they drain or change within a character cell. Resetting them during restore can alter the first resumed pixels or phase. Only the immutable timing configuration remains outside the payload; every owner reattaches the correct configuration after decode.

The shared engine and six wrappers use version 0.2.0 because their positional serialized layout changes. Spectrum runtime saves use version 17. Version 16 is rejected before payload decoding: it does not contain enough state for exact reconstruction. Do not silently default the missing fields or suppress the first resumed frame. Other legacy versions retain their existing rejection policy.

No chip timing or rendering algorithm changes. Regression evidence compares uninterrupted and restored pixels, palette, audio sample bits, timestamps and final complete saves at eight phases, all 13 profiles and four Timex video modes (176 combinations). The original engine fails on the first resumed Spectrum 16K frame; the original Timex capture separately reproduced eight changed pixels in the first frame on both boards in modes 0 and 6. These tests prove deterministic continuation of the current model, not independent hardware accuracy. Timex rendering and ROM boot qualification remain #52.

Drift triggers: skipping mutable pipeline fields to preserve old bytes; reconstructing a latch from its current input; accepting an old positional payload under a new schema version; treating successful re-encoding alone as proof of resumed execution.
