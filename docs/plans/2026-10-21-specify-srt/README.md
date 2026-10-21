# SPECIFY does not pace Seek

**Four cases fail the programmed-step invariant, twice. No operation is qualified.** [Emu#1970](https://github.com/emu198x/emu198x/issues/1970) owns the runtime defect. The source-build observations are pinned to `cacb63cfb78b080c681a5b1a436b4abb07447230`.

| SRT | Target from PCN0 | Observed completion ticks |
|---:|---:|---:|
| 1 | 1 | 256 |
| F | 1 | 256 |
| 1 | 2 | 256 |
| F | 2 | 256 |

The diagnostic supplies READY=true, allows100000 idle polling ticks, drains READY attention, and verifies no stale IRQ before seeking. Each Seek is bounded to2000000 ticks and must return exact SIS `[20,target]`. Its timer stops at IRQ assertion. MSR drive-busy remains set until SIS consumes ST0; falling busy is deliberately not the timer.

Run from an Emu198x repository using the immutable source-and-fixture commit below. It contains the captured controller source plus this diagnostic; later release version bumps cannot invalidate its captured path-dependency lockfile.

```sh
git fetch origin 171899d9eeb6f0aebbd91134ea885c9b1991a5e1
repro_dir="$(mktemp -d /tmp/198x-specify-srt.XXXXXX)"
git worktree add --detach "$repro_dir/source" 171899d9eeb6f0aebbd91134ea885c9b1991a5e1
CARGO_TARGET_DIR="$repro_dir/target" cargo run --locked --manifest-path "$repro_dir/source/docs/plans/2026-10-21-specify-srt/reproducer/Cargo.toml"
```

After the expected failure, remove that disposable checkout and cache:

```sh
git worktree remove "$repro_dir/source"
rm -r "$repro_dir"
```

Do not run the captured lockfile against an arbitrary newer checkout. The immutable commit includes the original dependency versions; the retained observations and hashes below describe the original executions, not a later release.

The captured current implementation exits101 with `SPECIFY SRT=1 must take longer than SRT=F; got 256 and 256`. This is an intentionally red standalone diagnostic outside the workspace test suite. It does not disable or skip a normal CI regression. After a fix, retain these receipts and move the invariant into ordinary passing tests; update any now-stale reproduction instructions explicitly.

`observation.json` identifies the source commit, executable, tools, environment, commands, artifact hashes, bounds and both fresh executions. Each raw stdout contains all four observations; stderr preserves the actual assertion failure. No firmware, media or snapshot was used. The experiment is a component source-build observation, not a distributed-release, ROM-loader or hardware result.

## Expectations and limits

The [NEC December1978 datasheet](https://bitsavers.org/components/nec/_dataSheets/uPD765_Data_Sheet_Dec78.pdf), printedpp15–16, defines SRT as adjacent-step spacing: `(16−SRT)` milliseconds at8MHz and twice that at4MHz. Thus SRT1 and F mean30ms and2ms at the +3 controller clock. The two-step cases make the missing adjacent interval visible even without settling first-pulse/completion offsets. Equal256-tick journeys violate both programmed spacing and distance dependence. The source explicitly discards SRT and immediately updates the target before a fixed countdown.

Inspected MAME `upd765.cpp::seek_continue` schedules each step with `500*(16−SRT)` rate cycles; Fuse `upd_fdc.c::seek_step` uses its programmed millisecond delay, doubled at4MHz. These are inspected implementation precedents, not fresh independent execution. MAME additionally models a2.5us step pulse and a10us Seek/20us Recalibrate final delay; Fuse steps immediately then waits its interval. Neither detail is silently promoted to a silicon oracle here.

Independent execution is available without hardware: reuse the existing unmodified Fuse controller/drive/event host and measure scheduled events at4MHz; use installed MAME0.289's existing +3 direct-port Lua probe with legitimate local firmware. Record event offsets and identities separately. These are digital controller experiments, not ROM-load qualification. Actual mechanical movement remains unmeasured.
