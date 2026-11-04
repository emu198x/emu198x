# Diagnose remaining native playback stalls

Engineering-frontier follow-up to the approved 50 ms shared playback reserve.
The previous native run still observed primed queue shortages. Do not claim
that queue priming alone makes the host stable or enlarge the reserve without
explaining what is consuming the time.

1. Recheck `crates/emu198x-ui/src/host_core.rs`, the winit host in
   `crates/emu198x-ui/src/lib.rs`, the shared native presenter and audio queue.
   Reuse `EMU198X_FRAME_STATS` for presentation summaries. Preserve unrelated
   `tools/__pycache__/` and the existing verification PRs.
2. Back up clean source, then add temporary correlated wall-time diagnostics
   for host wake gaps, slow runtime slices, slow presentation, catch-up budget
   exhaustion and primed audio shortages. Build release with the same original
   Apple II ROM/glyph fixture. Run for a bounded period; separate untouched
   idle time from UI interaction and other host load. No hardware clock changes.
3. Correlate shortages with measured delays. If necessary run a single narrow
   reversible comparison to distinguish presentation blocking from runtime
   throughput or host scheduling. A different architecture or user-visible
   presentation policy needs agreement before implementation.
4. Fix a demonstrated bug within the existing conventions, with a regression
   that fails under the old behaviour. Otherwise record the measured blocker
   and propose the smallest evidenced design choice. Do not guess a fix.
5. Restore all diagnostic-only source, run relevant tests/Clippy/format and
   published-source checks for any shipping change, commit/push evidence and
   verified changes, and remove task builds, bundle, log and processes. Keep
   physical listening qualification distinct from generated sample checks.


## Results, 2026-11-04

The remaining playback issue is open. No additional runtime or presentation
change was made. Temporary diagnostic source was restored to commit `3982bbe85`.
The release build with instrumentation succeeded.

Three window-free runs each completed 600 frames and 143,366,386 master ticks
in 1.44–1.51 seconds. See [headless measurements](2026-11-04-native-audio-stalls-evidence/headless.json).
This demonstrates average throughput headroom, not a bound on scheduling latency.

An original Apple II Monitor session ran without disk media or keyboard input
from approximately 16:24:40 to 16:29:06 UTC. No build ran alongside it. A five-second
`sample` of the task process was collected during this interval. The Mac reported
12 logical cores and load averages of 186.07 and later 368.78 before/during launch;
42% memory was reported free at the earlier check. This is a heavily loaded-host
observation, not a quiet-host baseline or an isolated presentation experiment.

The [filtered trace](2026-11-04-native-audio-stalls-evidence/trace.txt) contains:

- 30 callbacks that began primed but lacked a complete output block.
- 196 audio push gaps over 25 ms; the largest (1.69 seconds) occurred at launch.
- 208 host tick gaps over 25 ms.
- 118 presentation calls over 25 ms; maximum 169.501 ms.
- Two runtime calls over 25 ms (38.684 and 26.415 ms).
- 44 exhausted catch-up budgets that reanchored the host deadline.
- 51 frame-stat summaries, each covering 300 unique frame submissions.

For example, a shortage at timestamp 1793809538898891 followed 55.188 ms
without an audio push. A presentation call completed 42.379 ms later after
97.529 ms elapsed; the next audio push followed a 114.554 ms gap. Another
shortage was followed by a 163.495 ms host tick gap without a preceding slow
presentation entry. The 50 ms reserve cannot cover these absences. Wall time
inside a call includes OS preemption; the trace does not prove GPU blocking.
No new physical listening result was collected.

The existing four-frame catch-up limit then discards residual wall-time lag.
That policy has existed since the shared harness was introduced, so it is not
established as a regression. Removing it without an overload policy risks a
catch-up spiral on a runtime that cannot sustain real time. Hardware clocks
remain unchanged.

## Next validation and design boundary

Repeat this baseline when host load is lower before attributing the gaps to
presentation or claiming normal-use stability. Then compare one host pacing
change at a time, including overload behavior. A dedicated emulation worker
would separate runtime/audio production from presentation, but cannot guarantee
scheduling under arbitrary OS load; it is an architectural change requiring
agreement before implementation. Increasing the reserve is not justified by
this run. The previous queue fix remains independently verified, with audible
qualification still open.

The task-owned app, process, diagnostic source and build cache are removed after
recording these results. Firmware and raw process stacks are not included.
