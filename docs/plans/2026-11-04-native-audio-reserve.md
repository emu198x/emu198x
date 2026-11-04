# Keep a reserve in shared native audio playback

Engineering-frontier work. Approved by the user on 4 November 2026 after
hands-on debug and release playback both crackled. Callback measurements
showed missing samples being replaced with zero. This changes host buffering
only; the runtime's oscillator and generated waveform retain their timing.

1. In `crates/emu198x-shell/src/audio.rs`, derive a 50 ms startup/recovery
   threshold from the actual output sample rate and channel count. Cap it at
   the caller's existing maximum. Output silence without consuming queued
   samples until primed; re-prime after clear or underrun. Keep complete
   interleaved frames and bound storage even for oversized packets.
2. Add callback-level tests in that file for delayed stereo packets, short and
   large callbacks, sustained starvation/recovery, clear, format-dependent
   thresholds and bounded overflow. First reproduce the delayed-delivery
   failure against immediate consumption; show the regression goes red when
   immediate consumption is temporarily restored after the fix.
3. Bump the independently published shell patch and its workspace requirement;
   check the published-source gate and lockfile. Run shell tests, Clippy,
   format, shared UI and Apple II tests and generated status checks.
4. Build the optimised Apple II binary with original caller-owned fixtures.
   Measure callback starvation separately from intended priming silence and
   ask for a listening check with the finite 1 kHz Monitor program. Remove all
   diagnostics before committing. Record exact results and limits in the
   family's `docs/verification.md` and native verification plan.
5. Commit and push verified changes, update the existing verification PR to
   describe the final fix, and remove task build caches, bundle and processes.
   Do not claim the listening result passes without the user's report.

No new dependencies or public API changes. Archive paths named in RULES.md no
longer exist; the frozen donor trees are read-only references. This is a small
change to the existing queue rather than an archive port.

Implementation checks: the delayed-delivery regression failed against the old
queue with `wait for the reserve`: `[1.0, -1.0]` instead of `[0.0, 0.0]`.
It also failed when priming was temporarily bypassed, then passed after
restoration. All 306 shell unit tests, 42 UI tests, five Apple II adapter tests
and two fixture-free process tests pass. Shell Clippy, formatting, generated
status and the published-source gate pass; shell 0.44.3 is a pending release.
The other lockfile entries retain their original versions.

The optimised original-ROM window used 48 kHz stereo, a 4,800-sample reserve
and a 24,000-sample maximum. It played the finite eight-second test workload.
Some primed callbacks still ran short during host activity. The listening
report is pending; smooth audible playback is not claimed. Temporary native
counters are excluded from shipping source. Final cleanup stops the window
and removes its bundle, logs and task build caches after all checks finish.
