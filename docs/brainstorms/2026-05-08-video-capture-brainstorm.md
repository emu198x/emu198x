---
date: 2026-05-08
topic: video-capture
---

# Video capture (cross-emulator)

## What We're Building

A `RecordVideo`-style ScriptStep pair (`StartVideoRecording` /
`StopVideoRecording`) that captures the live framebuffer + audio
during the recording window and produces an MP4 file. Lives shell-side
so every emulator inherits it; per-system runtimes provide
RGB-converted frames via the existing `CapturedFrame` infrastructure.

Closes the long-standing want (alongside the screenshot capture
that already exists) and unblocks Code198x's curriculum video skills,
which today are stale `record_video` scripts targeting a JSON action
that never landed.

## Why This Approach

**Why ffmpeg subprocess** — every serious emulator with video capture
uses it (VICE, WinUAE, MAME, libretro, DOSBox); pure-Rust video codecs
that aren't GIF are a significant engineering investment with marginal
benefit; ffmpeg is universally available and Code198x's Docker pipeline
adds it in one line. Pure-Rust GIF was considered and rejected — Steve
wants the same capability across all emulators, and GIF is a
Spectrum-palette-only solution that doesn't generalise.

**Why shell-side upfront, not Spectrum-first port** — "all emulators"
is precisely the case where doing the layering once is cheaper than
porting four times. The cross-system encoding pipeline (ffmpeg pipe +
audio mux) lives in shell once; per-system runtimes contribute only
the palette-converter (~30 lines each, mostly already done).

**Why Start/Stop pair, not single-step `RecordVideo { frames }`** —
Code198x's curriculum-video pattern is "boot → type a BASIC program →
run it → start recording → run for 10 seconds → stop". Single-step
can't express the "do these things during recording" middle. The pair
adds session state but unlocks the only really useful video-capture
workflow.

**Why mux audio in same MP4** — Spectrum content has meaningful audio
(beeper music, AY tunes, tape loading sounds); video without sound is
a regression from "just works" UX. Slightly more implementation
(temp WAV + second ffmpeg call for the mux) but worth it.

## Key Decisions

- **Format**: MP4 / H.264 video / AAC audio. Conventional, universally
  browser-embeddable, both codecs ffmpeg-built-in.
- **Encoder**: ffmpeg subprocess. Detected via `which ffmpeg` at
  start-of-recording; clean error if missing.
- **Audio strategy**: muxed into the same MP4 from the start. Audio
  reuses the existing `AudioCapture` sink (already accumulates WAV
  bytes during emulation); written to a temp WAV at stop time;
  second ffmpeg call mux's video + audio into the final MP4.
- **API**: `StartVideoRecording { path: PathBuf }` and
  `StopVideoRecording` ScriptStep variants. Session gains
  `recorder: Option<VideoRecorder>` field. Frame-loop in `run_until`
  checks the field after each frame and writes the latest framebuffer
  (RGB-converted via `CapturedFrame`) to ffmpeg stdin.
- **Constraint model**: recording-aware. While recording is active,
  `SetMachine` / `LoadSnapshot` / nested `StartVideoRecording` are
  disallowed (each would discard or jump-cut machine state).
  Everything else is allowed and observable in the recording.
- **Scope**: shell-side `VideoRecorder` struct + the two ScriptStep
  variants land cross-emulator from day one. Spectrum gets its
  palette-converter wired immediately; C64 / NES / Amiga inherit the
  infrastructure when each is ready.
- **Frame rate / resolution**: native per system, derived from the
  runtime's frame timing. No user-configurable resolution in this
  commit.
- **Failure handling**: ffmpeg-not-found surfaces a clear error at
  Start; mid-recording ffmpeg failures capture stderr and surface in
  the StopVideoRecording observation.

## Open / parked items (not in this commit)

- **Audio-only stream codec** — `SaveAudioCapture` currently writes
  WAV (~5 MB/min). Switching to MP3 is a quality-of-life win for
  Code198x's audio-only assets but orthogonal to video capture.
- **Multi-recording or overlapping recordings** — disallowed for now;
  YAGNI.
- **Configurable bitrate / preset** — let ffmpeg defaults apply;
  exposable later if needed.
- **`Codec` field** on the JSON step — ditto, expose later if anyone
  needs WebM/VP9 instead of MP4/H.264.
- **`StopVideoRecording`'s observation** — emit
  `ScriptObservation::StopVideoRecording { path, frames, duration_ms }`
  so scripts can verify what was captured.

## Next Steps

→ Implementation. Phase shape:
  1. Shell-side `VideoRecorder` struct + ffmpeg subprocess wiring + tests.
  2. Two new ScriptStep variants in shell with the constraint guards.
  3. Session integration: per-frame teeing into the recorder.
  4. Spectrum binary's script module: wire the new steps through (no
     interceptor needed — these are shell-level steps, not
     system-specific like SetMachine).
  5. Smoke: capture a 5-second clip of the 48K boot sequence; verify
     the MP4 plays and has audio.
  6. Update Code198x's `scripts/emu-video-spectrum.sh` to use the new
     `StartVideoRecording` / `StopVideoRecording` JSON steps.
