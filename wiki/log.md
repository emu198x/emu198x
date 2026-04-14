# Wiki Log

Append-only record of ingests, queries, and lint passes.

---

## 2026-04-14 — 6502 core reaches full Harte, Lorenz, and Dormann green; Ghostbusters remains a C64-system issue

**Type:** milestone
**Trigger:** After the first 6502 verification slice, Ghostbusters still failed on the fresh-workspace C64 datasette path. That made it necessary to keep pushing CPU verification until the remaining gap was small enough to either explain the title failure or rule the CPU out cleanly.
**Result:** the live `mos-6502` core is now externally clean on all three currently wired verification families:
1. Completed the missing undocumented-opcode semantics in the live core for `ARR`, `ANC`, `ANE`, `LXA`, `LAS`, `SHA`, `SHX`, `SHY`, and `TAS`, instead of leaving them as `NopRead` placeholders.
2. Fixed a second real timing bug in the addressing-mode scheduler: zero-page and absolute indexed paths had been deciding whether they were indexed from the runtime index value (`X=0`/`Y=0`) instead of from the opcode's addressing mode, which broke both documented and undocumented timing on cases such as `ORA zp,X` and `ASL zp,X` when the index register happened to be zero.
3. Corrected the `ANE` unstable mask to match the external vectors and added a decimal-mode `ARR` path that matches the real NMOS behavior more closely instead of panicking in debug builds.
4. Added `tests/dormann_tests.rs`, wired to the packaged local Klaus Dormann functional memory image under `~/Projects/Emu198x-Unclean/6502_65C02_functional_tests`.
5. Extended test fixture discovery so Tom Harte, Lorenz, Dormann, and the C64 KERNAL ROM all resolve consistently from local external paths or explicit environment variables.
**Verification:** locally, the current state is:
- Tom Harte full NMOS 6502 corpus: `2,560,000 / 2,560,000` passed
- Lorenz CPU subset: `222 / 222` passed
- Dormann functional test: pass, reaching success loop `$3469` in `96,241,367` cycles
- `cargo test -p mos-6502`
- `cargo clippy -p mos-6502 --all-targets -- -D warnings`
**Consequence:** Ghostbusters still does not progress into a useful end state after the first tape stage even with the now-verified 6502 core, so the remaining issue is no longer credibly “the CPU is probably wrong.” The next debugging surface should be the wider C64 machine path: datasette stage transitions, CIA/VIC-visible loader behavior, and machine-level trace/query instrumentation.

## 2026-04-14 — 6502 verification harnesses land, and the first Tom Harte fix closes a real C64-relevant timing bug

**Type:** milestone
**Trigger:** `Ghostbusters` on the fresh-workspace C64 datasette path exposed a real gap in confidence: the tape loader reached `FOUND MAIN` / `LOADING`, then stalled without progressing into useful software state. At the same time, branch coverage had slipped, and the 6502 still lacked the same kind of external verification bar the Z80 already had.
**Result:** the 6502 now has real external verification harnesses, and they have already paid off:
1. Added `tests/single_step_tests.rs` to `mos-6502`, wired to the local Tom Harte NMOS 6502 corpus under `~/Projects/Emu198x-Unclean/65x02/6502/v1` (or `EMU198X_6502_TOM_HARTE_DIR`).
2. Added `tests/lorenz_tests.rs` to `mos-6502`, wired to the local Wolfgang Lorenz C64 CPU suite plus a real KERNAL ROM. The harness mirrors the archived C64-style trap setup instead of inventing a new execution environment.
3. Added fixture discovery helpers in `tests/support/mod.rs` for Tom Harte, Lorenz, and the local C64 KERNAL ROM.
4. Corrected the Lorenz harness budgeting so long-running ADC/SBC and flow/branch cases stop being misreported as CPU failures when they simply exceed an unrealistically low cycle cap.
5. Fixed a real documented-core bug in `tick_absolute`: absolute indexed reads without page crossing were incorrectly taking the longer page-cross path. Tom Harte documented-opcode failures for `ORA abs,X` / `ORA abs,Y` dropped to zero immediately after the fix.
**Verification:** locally, the current state is:
- Lorenz smoke `ldab`: pass (`14,259,836` cycles)
- Lorenz `adcb`: pass (`20,888,066` cycles)
- Lorenz CPU subset: `212 / 222` passed, with the remaining failures concentrated in undocumented-opcode families (`ARR`, `ANC`, `ANE`, `LXA`, `LAS`, `SHX`, `SHY`, `SHS`, plus one remaining long-running `ancb` path)
- Tom Harte full NMOS 6502 corpus: improved from `2,402,361 / 2,560,000` to `2,485,211 / 2,560,000` passing after the `tick_absolute` fix
- `Ghostbusters` still does not complete end-to-end, which is now better framed as a remaining CPU/machine-verification problem rather than a blind TAP-loader guess
**Next dependency:** keep driving Tom Harte and Lorenz until the remaining failure surface is small and named, then circle back to `Ghostbusters` and other real C64 titles with that stronger CPU baseline.

## 2026-04-14 — C64 datasette software validation now includes Thomas alongside Thinker

**Type:** milestone
**Trigger:** After the first ROM-backed C64 tape regression (`Thinker`) was in place, the next useful step was not more transport plumbing but a second real title with a fast, queryable, decoded screen state. Several local TAP candidates either stayed visually blank under text decoding or never settled into a useful automated stop condition, so the target needed to be chosen by actual headless probe rather than by filename.
**Result:** the fresh-workspace C64 now has a second ROM-backed datasette software regression:
1. Added a local TAP fixture helper for `Thomas the Tank Engine (1990)(Alternative Software)`.
2. Added an ignored runtime test that boots the real PAL C64 ROM set, inserts the Thomas TAP archive, drives the real `SHIFT+RUN/STOP` KERNAL autoload path, and proves the decoded screen reaches `FOUND THOMAS`, `LOADING`, and then `READY.` on the following line.
3. Updated the active README and C64 system note so the current software-validation state reflects both `Thinker` and `Thomas`, instead of implying there is only one real-title tape proof.
**Verification:** `cargo test -p runtime-commodore-c64 real_tap_autoload_reaches_thomas_loading_ready_banner -- --ignored --nocapture`, `cargo test -p runtime-commodore-c64`, and `cargo clippy -p runtime-commodore-c64 --all-targets -- -D warnings` should pass locally.
**Next dependency:** the next honest C64 media step is either one more well-chosen TAP title with a stronger end state, or moving on to the 1541/disk path once datasette coverage feels sufficient.

---

## 2026-04-13 — Native shell tape controls are standardized around play, stop, turbo, reset

**Type:** milestone
**Trigger:** After the first C64 native-shell tape pass, the live hotkeys had diverged in the wrong way. Spectrum still used `F5`-`F8`, while C64 had `F9`/`F10` plus an `F11` autoload macro. That made the host layer inconsistent and mixed startup workflow actions with ongoing transport actions.
**Result:** the current native verifier shells now share one temporary host-control layout, while keeping autoload as a startup workflow:
1. `emu198x-spectrum` now uses `F9` start, `F10` stop, `F11` turbo, and `F12` reset instead of the older `F5`-`F8` bindings.
2. `emu198x-c64` keeps `F9` start, `F10` stop, and `F12` reset, but `F11` now toggles cycle-faithful tape turbo instead of triggering a live autoload macro.
3. Added `--turbo-tape` to `emu198x-c64`, matching the existing Spectrum startup option so both verifier shells can launch with tape turbo armed.
4. Left `--autoload-tape` in place for both families as a startup-only host workflow over the real ROM/KERNAL path, not a normal live transport control.
**Verification:** `cargo fmt --all --check`, `cargo test -p emu198x-c64 -p emu198x-spectrum`, `cargo clippy -p emu198x-c64 -p emu198x-spectrum --all-targets -- -D warnings`, and `cargo run -p emu198x-c64 -- --help` / `cargo run -p emu198x-spectrum -- --help` should pass.
**Next dependency:** if a later family needs different raw keys, standardize the host actions instead of forcing the same unmodified key range across machines.

---

## 2026-04-13 — Native C64 tape controls now reuse the same runtime autoload path

**Type:** milestone
**Trigger:** Once the headless C64 path could insert TAP media, drive real `SHIFT+RUN/STOP`, and prove a ROM-backed Thinker load, the native verifier shell still lagged behind. It could boot and import host-side files, but it had no equivalent tape insertion or autoload workflow above the same runtime boundary.
**Result:** `emu198x-c64` now speaks the same tape control language as the headless runner instead of inventing a UI-only path:
1. Added startup `--tape`, `--autoload-tape`, and `--start-tape` handling in the native shell, with the same conflict rules and media loading semantics as `emu198x-script-c64`.
2. Added live `F9` start, `F10` stop, and `F11` autoload controls. `F11` routes through `runtime-commodore-c64::autoload_basic_tape()` over a temporary `HeadlessSession`, so it still drives the real KERNAL prompt and datasette transport instead of synthesizing machine state.
3. Updated the native window title and shell help so tape state is visible (`tape playing`, `tape loaded`, or `no tape`) and the verifier workflow is discoverable.
4. Added CLI coverage for the new tape flags in `emu198x-c64` tests.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p runtime-commodore-c64`, `cargo clippy -p emu198x-c64 --all-targets -- -D warnings`, and `cargo run -p emu198x-c64 -- --help` all pass locally.
**Next dependency:** the next honest C64 tape milestone is a native-shell regression target or additional real software validation above the same TAP path, not a second UI-only media flow.

---

## 2026-04-13 — C64 tape autoload now drives the real KERNAL path, and Thinker reaches post-load READY.

**Type:** milestone
**Trigger:** Once TAP playback was wired through the 6510 and `CIA1 FLAG`, the remaining question was not whether pulses existed but whether the actual C64 ROM workflow could use them. We needed a host helper over the real machine path and at least one concrete software proof above raw transport control.
**Result:** the fresh-workspace C64 now has a real tape-autoload workflow and a ROM-backed tape regression:
1. Added `runtime-commodore-c64::autoload_basic_tape()`, which waits for `READY.`, presses the real `SHIFT+RUN/STOP` KERNAL shortcut, waits for `PRESS PLAY ON TAPE`, and only then starts `tape-1`.
2. Added decoded `screen.text.lines` plus `boot.row` to the C64 query provider, so scripting and future MCP automation can observe text-mode KERNAL states directly instead of relying on screenshots.
3. Wired `--autoload-tape` into `emu198x-script-c64`, keeping it explicitly separate from raw `--start-tape`.
4. Added a new ignored ROM-backed runtime test against the local `Thinker, The (1984)(Atlantis)` TAP archive. The current proof reaches `FOUND THINKER`, `LOADING`, and then a second post-load `READY.` line under the real KERNAL tape path.
**Verification:** `cargo test -p runtime-commodore-c64 -p emu198x-script-c64`, `cargo clippy -p runtime-commodore-c64 -p emu198x-script-c64 --all-targets -- -D warnings`, and `cargo test -p runtime-commodore-c64 real_tap_autoload_reaches_post_load_ready -- --ignored --nocapture` all pass locally. The stronger ignored `Thinker` proof completed in `43.37s`.
**Next dependency:** the next honest C64 tape milestone is either a full end-of-load software regression on a tractable TAP title or native-shell tape insertion/control above the same runtime path.

---

## 2026-04-13 — C64 T64 support lands as a host-side container import, not fake datasette media

**Type:** milestone
**Trigger:** With TAP now living on the real datasette path, the separate `T64` request needed to be handled without eroding the repo’s “no shortcuts” accuracy bar.
**Result:** `T64` support now exists, but on the correct side of the emulation boundary:
1. Added `format-commodore-c64-t64`, which parses `T64` headers and extracts the first loadable entry as a PRG byte stream.
2. Extended the shell asset loader so zipped or plain `.t64` files are recognized as host-side program assets.
3. Updated `runtime-commodore-c64::file_loader` so `--load demo.t64` imports the first loadable archive entry into RAM, just like a host-side PRG convenience path.
4. Kept the boundary explicit in code and docs: `T64` is a container import path under `--load`, not a claim of pulse-timed datasette playback.
**Verification:** `cargo test -p format-commodore-c64-t64 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-shell` and `cargo clippy -p format-commodore-c64-t64 -p runtime-commodore-c64 -p emu198x-script-c64 -p emu198x-shell --all-targets -- -D warnings` both pass locally.
**Next dependency:** if we want richer `T64` handling later, the next honest extension is entry selection and metadata exposure, not pretending the container is equivalent to raw TAP pulse media.

---

## 2026-04-13 — C64 datasette TAP path is live through the 6510 and CIA1 FLAG

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 could boot to `READY.`, emit video/audio, and import host-side `.prg` / `.bas` files, the biggest remaining honesty gap on the software-loading side was obvious: the datasette slot existed in profile metadata, but there was still no real tape medium on the board path.
**Result:** the first honest C64 tape path now exists:
1. Added `format-commodore-c64-tap`, which parses Commodore TAP headers and pulse streams into native machine-cycle pulse lengths rather than flattening them into a fake file loader.
2. Added a board-local datasette component to `machine-commodore-c64`, serialized in snapshots and advanced one `phi2` cycle at a time.
3. Wired datasette sense and motor semantics into the 6510 `$0001` port boundary, so PLAY state and motor control live on the machine path instead of in the runtime.
4. Wired datasette flux changes into `CIA1 FLAG`, which is the real interrupt-visible read path the C64 uses for tape input.
5. Updated `runtime-commodore-c64` and `emu198x-script-c64` so `tape-1` now supports media load plus start/stop transport control and exposes `c64.tape.loaded` / `c64.tape.playing` queries.
6. Kept scope honest: this is TAP only for now. `T64` remains a separate follow-up format and is not being misrepresented as pulse-accurate datasette media.
**Verification:** `cargo fmt --all`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` should pass. New coverage includes TAP parser tests, board tests for 6510 sense + motor gating + CIA1 FLAG delivery, and runtime/runner integration staying green under the broader workspace gates.
**Next dependency:** the next meaningful C64 media step is real software validation on this datasette path, then either native-shell tape insertion/control or the 1541/disk side once the second-computer scope is justified.

---

## 2026-04-13 — Native Spectrum and C64 shells now step in sub-frame slices

**Type:** milestone
**Trigger:** After the first C64 verifier-shell pass, keyboard input still felt soft even once the immediate wake-up fix landed. The remaining problem was host scheduling granularity: both native shells still advanced the machine only one full frame at a time outside turbo mode, so host input was effectively quantized to frame boundaries.
**Result:** both native verifier shells now step the machine in smaller real-time slices while keeping presentation at real frame completion:
1. Added sub-frame scheduling to `emu198x-c64`, so normal execution now advances in 1/8-frame slices instead of whole-frame chunks. Input events are applied on the next slice, but redraw still waits for a real new machine frame.
2. Applied the same change to `emu198x-spectrum` outside tape-turbo mode, so both native shells now share the same lower-latency host loop shape.
3. Kept timing honest: the shells still use the same underlying machine clocks and frame cadence; only host wake-up granularity changed.
4. Added shell-local timing-budget tests so future refactors do not quietly collapse the slice scheduler back to full-frame stepping.
5. The shells remain verifier-grade rather than polished frontends. Subjective input feel is improved but still softer than target, so the remaining work is documented rather than silently implied away.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p emu198x-spectrum`, and `cargo clippy -p emu198x-c64 -p emu198x-spectrum --all-targets -- -D warnings` should pass.
**Next dependency:** if input still feels soft after this, the next place to look is not the host scheduler but machine-side keyboard handling cadence under the ROM/KERNAL paths themselves.

---

## 2026-04-13 — Native shell naming is prefixed and C64 now has a verifier window

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 had live 6502/CIA/VIC-II/SID execution, boot detection, snapshots, and host-side program import, the next practical gap was the same one Spectrum had already exposed: a thin native verifier shell makes real-machine checking dramatically faster. At the same time, the shell naming had drifted toward short `emu-*` package names even though cross-project infrastructure already uses the `emu198x-*` prefix.
**Result:** the native shell surface is now consistent and the C64 has its first windowed runner:
1. Added `emu198x-c64`, a thin `winit` + `pixels` native verifier shell over `runtime-commodore-c64`. It boots PAL or NTSC breadbin profiles through the shared shell/runtime boundary, renders the live VIC-II framebuffer, plays the runtime's mono audio stream, forwards keyboard input into the real matrix, and supports hard reset plus optional startup snapshot/program import.
2. Expanded the C64 host key namespace in `runtime-commodore-c64` so the native shell can drive real function keys, cursor aliases, delete/home, control keys, and common punctuation without inventing a second input path.
3. Renamed the Spectrum native shell from `emu-spectrum` to `emu198x-spectrum` at both the package and crate-path level, so the workspace and public runner name now agree on the `emu198x-*` prefix.
4. Updated the active README/frontend/docs surface so current commands and crate names point at `emu198x-spectrum` and `emu198x-c64` instead of the shorter `emu-*` forms.
**Verification:** `cargo fmt --all`, `cargo test -p emu198x-c64 -p runtime-commodore-c64 -p emu198x-spectrum`, `cargo run -p emu198x-c64 -- --help`, and `cargo run -p emu198x-spectrum -- --help` should pass before the full workspace gates.
**Next dependency:** with both Spectrum 48K and the C64 now having working native verifier shells, the next honest C64 step is real media hardware rather than more shell scaffolding.

---

## 2026-04-13 — C64 SID is live and the runtime now emits audio

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 could boot to `READY.` and import host-side programs, the biggest remaining board-level dishonesty was obvious: the SID was still only a register shadow, so the machine claimed a C64 audio path without actually modelling or emitting one.
**Result:** the fresh-workspace C64 now has a real SID and end-to-end runtime audio:
1. Added `mos-sid-6581` to the workspace with the archived three-voice oscillator, ADSR envelopes, state-variable filter, and downsampled audio buffer, including its 9-chip tests.
2. Replaced `machine-commodore-c64`'s shadowed SID register array with a live `Sid6581`, clocked once per `phi2` tick and serialized as part of machine snapshots.
3. Kept the board boundary honest: `$D400-$D7FF` now routes to the real SID register bus, and the machine exposes a real mixed audio buffer instead of pretending that writes alone constitute sound support.
4. Updated `runtime-commodore-c64` so frame execution now also drains the SID buffer and emits mono audio packets through the shared shell audio sink.
5. Tightened the fresh-workspace summaries so the C64 no longer describes SID as shadowed in the runtime/profile/readme layer.
**Verification:** `cargo test -p mos-sid-6581 -p machine-commodore-c64 -p runtime-commodore-c64`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. New coverage includes C64 board tests for live SID register writes and generated audio samples, plus a runtime test proving one frame now emits both RGBA video and mono audio packets.
**Next dependency:** the next C64 step is still not more shell work. It is either a minimal native verifier shell over this now-audible runtime, or the first honest media path once the corresponding hardware is modelled far enough to deserve the claim.

---

## 2026-04-13 — C64 headless runner now imports PRG and plain-text BASIC

**Type:** milestone
**Trigger:** Once the fresh-workspace C64 had a booted runtime, snapshots, and a headless runner, the next practical gap was software injection. The immediate need was developer-grade program loading, including plain-text BASIC source rather than only pre-tokenised artifacts.
**Result:** the C64 host workflow now has an explicit software-import path above the runtime boundary:
1. Added `format-commodore-c64-prg`, which parses PRG files, imports them into RAM through a narrow `RamAccess` trait, and relinks BASIC pointers when the load address is `$0801`.
2. Added `format-commodore-c64-bas`, which tokenises UTF-8 plain-text Commodore BASIC source into PRG bytes without pretending that source import is a shared cross-family format concern.
3. Added `runtime-commodore-c64::file_loader`, which treats `.prg` and `.bas` as host-side convenience imports over the live machine instead of as emulated media devices.
4. Wired `--load PATH` into `emu198x-script-c64`. The runner now waits for `READY.` automatically before importing, so BASIC/KERNAL startup does not immediately overwrite the injected program.
5. Kept the boundary honest in the docs: this is not tape or disk support, and Spectrum will need its own family-specific text-import path later because tokenisation rules differ.
**Verification:** `cargo test -p format-commodore-c64-prg -p format-commodore-c64-bas -p machine-commodore-c64 -p runtime-commodore-c64 -p emu198x-script-c64`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. A real runner proof also succeeds locally with `emu198x-script-c64 --load /tmp/emu198x-demo.bas --save-snapshot /tmp/emu198x-demo.c64.pst`.
**Next dependency:** the next honest C64 software path is real media support, starting with PRG-friendly disk or tape workflows only when the corresponding hardware path is modeled directly enough to deserve the claim.

---

## 2026-04-13 — C64 snapshots and the first fresh-workspace headless runner land

**Type:** milestone
**Trigger:** Once the PAL C64 could boot real ROMs through the shared runtime, the next useful host-facing step was a thin runner and honest snapshot support. The runtime could not claim save states safely until the chip state serialization itself was complete.
**Result:** the fresh workspace now has a usable C64 automation path instead of just an internal runtime:
1. Added `emu198x-script-c64` as the first fresh-workspace C64 headless runner, with ROM directory resolution, PAL/NTSC model selection, boot waits, shared JSON script execution, PNG screenshots, and snapshot load/save.
2. Added real runtime snapshot import/export to `runtime-commodore-c64` using the same `postcard` envelope pattern as the Spectrum runtime.
3. Added machine-local `C64Snapshot` and `C64MemorySnapshot` state capture/restore in `machine-commodore-c64`, so the runtime is not trying to serialize board state ad hoc.
4. Tightened snapshot fidelity by serializing the 6502's in-flight cycle state and the VIC-II framebuffer instead of silently dropping them at snapshot boundaries.
5. Updated profile metadata and scripting docs so C64 now advertises runtime snapshot support honestly, while still leaving media support out of scope.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass. The new C64 runtime test exercises snapshot round-tripping mid-cycle, and the runner can boot or restore from the local ROM set at `~/.emu198x/roms/commodore-c64`.
**Next dependency:** the next honest C64 step is media and software workflows above this runner boundary, not more shell scaffolding.

---

## 2026-04-13 — C64 runtime now emits frames and detects the READY. boot state

**Type:** milestone
**Trigger:** Once the fresh C64 machine could boot real ROMs to `READY.` inside the machine crate, the next honest step was to push that proof through the shared shell boundary instead of leaving C64 as metadata plus chip tests.
**Result:** the fresh workspace now has its first real C64 runtime surface:
1. Added `C64Runtime` in `runtime-commodore-c64`, backed by the live `machine-commodore-c64` board and the real BASIC/KERNAL/CHARGEN firmware set.
2. `run_until()` now advances the C64 in authoritative `phi2` cycles and emits RGBA framebuffer packets to the shared host sinks, so the family is no longer “catalogue only.”
3. Added `C64SessionQueryProvider` with a minimal boot/query namespace: `boot.detected`, `boot.reason`, `boot.offset`, plus current raster/IRQ/BA state.
4. Added two ROM-backed proofs of the visible boot path:
   - an ignored machine test that boots the PAL C64 and finds `READY.` in screen RAM
   - an ignored runtime test that drives the same ROM set through `run_until()` and resolves `boot.detected = true`
5. Tightened the profile honesty: PAL now reports `SupportTier::Boots`; NTSC stays at `Research` until it is verified on the same footing.
**Verification:** `cargo test -p machine-commodore-c64 boots_kernal_to_ready_prompt -- --ignored --nocapture`, `cargo test -p runtime-commodore-c64 query_provider_detects_ready_on_real_pal_boot -- --ignored --nocapture`, `cargo test --workspace`, and `cargo clippy --workspace --all-targets -- -D warnings` all pass locally.
**Next dependency:** the next C64 step is a thin runner above this runtime, then PRG/media support and SID if the booted machine path stays stable under real software.

---

## 2026-04-13 — Live VIC-II replaces the C64 board shadow registers

**Type:** milestone
**Trigger:** After the 6502 and both CIAs were live, the main remaining board-local fake on the C64 path was the VIC-II. Keeping register shadows any longer would have stalled the bring-up at exactly the point where BA, raster IRQs, and framebuffer ownership start to matter.
**Result:** the fresh-workspace C64 now owns a real VIC-II chip instead of a board-local placeholder:
1. Added `mos-vic-ii` with the archived raster state machine, badline detection, sprite BA lead-in, register bus, raster IRQs, light-pen latch, visible framebuffer, and text/bitmap/sprite render paths.
2. Implemented `mos_vic_ii::VicMemory` for `C64Memory`, so VIC-visible banked RAM, character ROM windows, and colour RAM now flow through the real machine memory router rather than through chip-local test hooks.
3. Reworked `machine-commodore-c64` to own a live `Vic`, route CIA2 bank selection into it, OR its IRQ into the CPU IRQ line, and gate CPU reads with `BA -> RDY` instead of assuming the CPU is always ready.
4. Kept the scope honest: SID is still shadowed, sprite and text rendering use the archived batch-fetch compromise, and the fresh workspace still does not claim a booted KERNAL. But the board no longer fakes the VIC side of the machine.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass, including the new `mos-vic-ii` unit suite and C64 board tests for VIC raster IRQ and badline BA read stalling.
**Next dependency:** the next C64 step is to keep wiring toward a real boot path: tighten the VIC/CPU interaction where the new board loop exposes gaps, then bring in SID and the first runtime shell once the machine can show meaningful KERNAL output.

---

## 2026-04-13 — Live CIA chips replace the C64 board shadows

**Type:** milestone
**Trigger:** The C64 board had reached real CPU-driven execution, but keyboard scan, bank selection, and interrupt sources were still hand-rolled shadows. The next honest step had to replace those with live chip behaviour instead of adding more board-local fakes.
**Result:** the fresh C64 path now owns real CIA chips:
1. Added `mos-cia-6526` with live ports, DDR masking, Timer A/B countdown, ICR mask/status handling, TOD divider logic, serial-shift completion tracking, and IRQ pin output.
2. Replaced `machine-commodore-c64`'s CIA shadow latches with two live `Cia6526` instances. CIA1 now owns keyboard-facing port state and IRQ generation; CIA2 now owns VIC bank-select port state and the NMI-side interrupt source.
3. Reworked the board tick order so keyboard scan feeds CIA1 before the chip ticks, both CIAs tick once per `phi2`, VIC bank selection is refreshed from CIA2's live PA pins, and the 6502 sees real CIA-driven `irq`/`nmi` levels.
4. Kept the scope honest: VIC-II and SID are still shadowed, but the machine no longer fakes the CIA side of the board.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass, including C64 board tests proving CIA keyboard scan, bank selection, and CPU interrupt-line routing.
**Next dependency:** the next C64 step is the VIC-II, because that is now the main missing live chip between this board loop and a real KERNAL boot path.

---

## 2026-04-13 — Fresh-workspace C64 now has a real 6502 and CPU-driven board loop

**Type:** milestone
**Trigger:** The C64 board substrate had real banking, keyboard scan state, and timing, but it was still only a timed board shell. The next honest step had to be a real processor on the bus.
**Result:** the fresh workspace now has the first CPU-driven C64 slice instead of only static board behaviour:
1. Added `mos-6502` as a standalone cycle-accurate pin-level CPU crate with opcode decode, per-cycle bus scheduling, decimal-mode support, and core execution tests.
2. Reworked `machine-commodore-c64` to own a real 6502, reset it through the KERNAL reset vector, and drive one actual CPU bus transaction per `phi2` cycle over the existing 6510-style memory banking and I/O shadows.
3. Kept the board scope honest: IRQ/NMI/RDY sources are still placeholders until live CIA/VIC models arrive, but the CPU is now executing through the real memory map rather than through synthetic state changes.
4. Added machine-level proofs that the board boots through the reset vector to the first opcode fetch and can execute `LDA`/`STA` through the board bus into RAM and I/O-visible register space.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass with the new `mos-6502` crate and the CPU-driven C64 board loop.
**Next dependency:** the next C64 step is to replace the current CIA/VIC shadows with live chip crates and start surfacing real interrupt, bank-select, and bad-line behaviour through the same board loop.

---

## 2026-04-13 — C64 machine substrate now has real banking, keyboard, and timing

**Type:** milestone
**Trigger:** The profile/timing bootstrap made the C64 family visible in the fresh workspace, but the next useful slice had to stop being metadata and start becoming board behaviour the future boot path can actually stand on.
**Result:** the new `machine-commodore-c64` crate now owns the first durable machine substrate for the fresh workspace:
1. Added a real 6510 memory subsystem with `$00`/`$01` port semantics, BASIC/KERNAL/character ROM visibility rules, colour RAM, and VIC-visible character ROM banking.
2. Added the 8×8 keyboard matrix as a pure active-low scan surface.
3. Added a minimal board loop with `phi2` cycle counting, raster/frame progression for PAL and NTSC, CIA-side keyboard scan latches, CIA2-driven VIC bank selection, and shadowed VIC/SID register storage for I/O-visible accesses.
4. Kept the scope honest: no 6502 core, no live VIC-II/CIA/SID behaviour, and no runtime yet. This is substrate only, not a runnable C64.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass with the new crate wired into the workspace.
**Next dependency:** the next real C64 step is the processor/chip wave above this substrate, starting with the 6502/6510 side and then replacing the current CIA/VIC register shadows with live chip models.

---

## 2026-04-13 — C64 bootstrap lands as timing and profile truth only

**Type:** milestone
**Trigger:** Spectrum 48K is now strong enough that the next architectural test should be a second family, but the repo also carries stale C64 status documents from older workspaces that would make it too easy to overstate progress.
**Result:** the fresh workspace now has an honest C64 bootstrap instead of another archive-shaped mirage:
1. Added `common-commodore-c64` with baseline PAL and NTSC breadbin timing constants: φ2 clock, raster geometry, CIA TOD dividers, and archived VIC-II capture window dimensions.
2. Added `runtime-commodore-c64` as the new family catalogue crate. It currently exposes PAL and NTSC breadbin research-tier profiles only, with three required ROMs and baseline tape/disk/cartridge slots.
3. Chose `phi2-cycle` as the authoritative profile clock for now instead of inventing a master-clock contract before the fresh workspace has a live VIC-II/6510 timing loop to anchor it.
4. Added a historical warning at the top of `wiki/systems/commodore-c64.md` so the old archived implementation-status text stops reading like current repo truth.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should pass once the new crates are wired in.
**Next dependency:** the next real C64 slice is no longer more metadata. It is the first machine-facing substrate: 6510/port banking boundaries, keyboard matrix state, and the minimal VIC-II/CIA-aware clock model that a future boot path can actually stand on.

---

## 2026-04-13 — Shared boolean query waits land, with a Spectrum tape-stop alias

**Type:** milestone
**Trigger:** After the UI got cycle-faithful tape turbo, the next practical automation gap was obvious: scripts and headless runs still had no clean way to block on a tape load finishing except by guessing a frame count.
**Result:** the shared shell now has a reusable boolean wait primitive, and the Spectrum headless runner exposes the tape-stop case directly:
1. Added `HeadlessSession::wait_for_query_bool(path, expected, max_frames)` plus a typed `QueryBoolWaitResult`.
2. Added `wait_for_query_bool` to the shared JSON script surface so automation can wait on boolean machine state without inventing ad hoc polling loops.
3. Added `--wait-for-tape-stop N` to `emu198x-script-spectrum` as a Spectrum-specific alias for waiting until `spectrum.tape.playing == false`.
4. Kept the alias ordered after autoload and script execution, so one command line can autoload a tape, wait for playback to finish, and then continue with any extra frame or capture steps.
**Verification:** `cargo test -p emu198x-shell -p emu198x-script-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** if we want to push this pattern further, the next useful addition is a more general query-equality wait only when a concrete workflow needs it. Right now boolean state and text containment cover the real use cases.

---

## 2026-04-13 — Spectrum verifier shell now has cycle-faithful tape turbo

**Type:** milestone
**Trigger:** Once tape autoload existed, the next quality-of-life gap was purely host-side: the native Spectrum shell still ran at wall-clock speed during long tape loads, even though the project explicitly rejects fake instant-load shortcuts.
**Result:** `emu-spectrum` now has a tape-only turbo mode that preserves exact machine execution:
1. Added `--turbo-tape` at launch and `F8` at runtime to arm or toggle tape turbo in the native verifier shell.
2. Turbo mode only engages while the tape is actually playing. When active, the host loop stops sleeping and runs bounded batches of real Spectrum frames as fast as the machine can execute them.
3. The implementation does not skip pilot tones, alter TZX/TAP semantics, or bypass ROM/tape behaviour. It is only a host scheduling change over the same machine path.
4. The window title now reports when turbo is armed or active so manual verification stays legible.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p emu-spectrum`, and `cargo test --workspace` all pass.
**Next dependency:** if we want more tape QoL after this, the next honest step is a “wait until tape stops / wait until query matches” convenience above the current headless session surface, not any form of instant loader.

---

## 2026-04-13 — Spectrum tape autoload is now a real host workflow

**Type:** milestone
**Trigger:** After Manic Miner and Jet Set Willy were both loading end to end, the next obvious friction point was that every real tape workflow still depended on hand-authored `LOAD ""` key choreography in tests or manual typing in the UI.
**Result:** the fresh Spectrum path now has a reusable tape autoload helper above the runtime boundary instead of burying that sequence inside ignored tests:
1. Added `runtime-sinclair-zx-spectrum::autoload_basic_tape()`, which waits for the 48K boot banner, exposes the BASIC prompt if needed, types the real `LOAD ""` keyword sequence through the ROM editor, and then starts tape transport on `tape-1`.
2. Kept that helper honest: it operates through `HeadlessSession`, shared input events, and normal media transport commands. It does not patch ROM code, skip leader tones, or bypass tape decoding.
3. Added `HeadlessSession::into_machine()` so the native verifier shell can reuse the same startup workflow without introducing a parallel machine-control path.
4. Wired `--autoload-tape` into both `emu198x-script-spectrum` and `emu-spectrum`, and added clear runner-level errors for missing tape media or conflicting `--play-tape` usage.
5. Replaced the duplicated hand-typed `LOAD ""` sequence in the ignored Manic Miner and Jet Set Willy ROM-backed regressions with the new helper.
**Verification:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_manic_miner_from_zipped_tzx -- --ignored --nocapture`, and `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_jet_set_willy_from_zipped_tzx -- --ignored --nocapture` all pass locally.
**Next dependency:** the next quality-of-life improvement worth doing is turbo loading as uncapped exact execution on the real machine path, not an instant-load shortcut.

---

## 2026-04-13 — Jet Set Willy joins the ROM-backed Spectrum software regressions

**Type:** milestone
**Trigger:** After Manic Miner became a real end-to-end tape regression, the next useful Spectrum software target needed to cover a different post-load path instead of just another early title-screen success.
**Result:** `runtime-sinclair-zx-spectrum` now has a second ignored ROM-backed tape regression for the original Jet Set Willy TZX:
1. Added a local fixture lookup for `Jet Set Willy (1984)(Software Projects).zip`.
2. Added an ignored test that boots the real 48K ROM, types `LOAD ""`, starts the tape, and waits for the copy-protection code screen text `Enter Code at grid location`.
3. Verified that this is a better automated target than the cracked image for now. The original build exposes a stable decoded text screen; the cracked build reaches a stopped post-load state, but its stylized title screen is not currently a strong text-query target.
**Verification:** `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_jet_set_willy_from_zipped_tzx -- --ignored --nocapture`, `cargo test -p runtime-sinclair-zx-spectrum`, and `cargo clippy -p runtime-sinclair-zx-spectrum --all-targets -- -D warnings` all pass locally.
**Next dependency:** Knight Lore and Atic Atac are still good manual/UI targets, but they need either a later post-load assertion or a different observable than decoded text before they become strong headless regressions.

---

## 2026-04-13 — Spectrum verifier shell now plays live beeper and tape audio

**Type:** milestone
**Trigger:** Once the Spectrum UI shell could boot and load real tape software, the next gap was obvious during manual verification: the frontend was still discarding the runtime’s audio packets, so there was no beeper output and no tape screech while loading.
**Result:** `emu-spectrum` now owns a real host-side audio path over the existing machine contract:
1. Added a `cpal`-backed audio sink in the frontend crate, using the default host output device and a bounded host-side queue.
2. Threaded the live audio sink through `SpectrumRunner::run_frame` instead of `NullAudioSink`, without changing the runtime or machine audio contract.
3. Kept conversion policy in the host layer: mono Spectrum packets are duplicated across the device channel count, and output-rate mismatch is handled by a small frontend resampler rather than by touching machine timing.
4. Added frontend-local tests covering mono-to-stereo duplication and the simple downmix/resample path.
**Verification:** `cargo test -p emu-spectrum`, `cargo clippy -p emu-spectrum --all-targets -- -D warnings`, and `cargo fmt --all --check` all pass.
**Next dependency:** the next useful frontend follow-up is host-configurable audio/input policy, not more machine-facing work in this area.

---

## 2026-04-13 — Spectrum tape loading now handles TZX pauses as timing spans, and Manic Miner loads end to end

**Type:** milestone
**Trigger:** Manual Spectrum verification had reached the first real software path, and Manic Miner was failing late with `R Tape loading error, 20:6` after the striped loader phase. That ruled out trivial command-entry mistakes and pointed toward the real tape path.
**Result:** the fresh Spectrum tape stack now models the missing semantics cleanly enough to load the zipped local Manic Miner fixture under the real 48K ROM:
1. Replaced the old flat pulse-only tape representation with a shared timing-span stream in `common-sinclair-zx-spectrum`. The tape player now supports edge-delimited pulses, held-level spans, and explicit stop markers, which are needed for TZX pause and direct-recording semantics.
2. Updated `format-sinclair-zx-spectrum-tzx` to parse TZX blocks into that richer span stream, including direct recording, pause blocks, signal-level directives, and loop expansion.
3. Corrected the machine-level tape override on port `$FE` bit 6 to follow the repo’s own Spectrum docs when tape is connected: external EAR high now reads as bit 6 low, and EAR low reads as bit 6 high.
4. Re-ran the real 48K ROM + zipped Manic Miner path headlessly and confirmed that the title now loads end to end. The successful run reached `screen.text.lines` containing `MANIC MINER` on row 19 after `10,672` frames of tape playback.
5. Added a new ignored local regression in `runtime-sinclair-zx-spectrum` that boots the real 48K ROM, types `LOAD ""` with the timing the Spectrum editor actually accepts, starts tape transport, and waits for the Manic Miner title screen from the zipped TZX fixture.
**Verification:** `cargo test -p common-sinclair-zx-spectrum`, `cargo test -p format-sinclair-zx-spectrum-tzx`, `cargo test -p machine-sinclair-zx-spectrum-48k`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, and `cargo test -p runtime-sinclair-zx-spectrum spectrum_boots_and_loads_manic_miner_from_zipped_tzx -- --ignored --nocapture` all pass locally with the required ROM and tape assets present.
**Next dependency:** the next useful pass is to turn the brittle Spectrum command-entry sequence into a higher-level host helper for scripting and MCP, so media autoloading does not depend on hand-authored key choreography.

---

## 2026-04-13 — Minimal native Spectrum verifier shell is live

**Type:** milestone
**Trigger:** The headless Spectrum path had reached the point where the next bottleneck was not another query or script primitive. It was the lack of a human-verifiable native frontend for real-time manual checking. At the same time, zipped local assets were already becoming normal, so the first UI needed to speak that host-side asset boundary instead of bypassing it.
**Result:** the repo now has a first native UI runner in `crates/emu-spectrum`:
1. Added `emu-spectrum`, a thin `winit` + `pixels` desktop shell over the existing 48K runtime. It boots the same `Spectrum48kRuntime`, renders the real indexed framebuffer in a native window, and drives the machine at Spectrum frame cadence instead of inventing a separate execution path.
2. Wired the UI shell to the shared asset boundary. `--rom` and `--tape` now accept plain files or zip archives with one matching candidate, using the same host-side asset loading code as the headless path.
3. Added practical live controls: host keyboard mapping for the Spectrum matrix, `Esc` to quit, `F5` hard reset, `F6` tape start, `F7` tape stop, cursor-key combos via `Caps Shift`, and `Alt` as `Symbol Shift`.
4. Kept the earlier ZIP-media work but narrowed the unfinished Manic Miner experiment back to a safe local smoke test. The fresh runtime now has an ignored test that verifies the zipped Manic Miner TZX fixture loads into the tape slot, without pretending the full autoload path is solved yet.
5. Corrected the frontend docs so they no longer claim that all family frontends already exist, while also no longer pretending there is no native frontend at all.
**Verification:** `cargo run -p emu-spectrum -- --help`, `cargo test -p emu-spectrum`, `cargo test -p emu198x-shell`, `cargo test -p emu198x-script-spectrum`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** the highest-value follow-up is to use this real windowed shell to debug the first honest software-loading path, especially the `LOAD ""` and Manic Miner tape workflow, rather than guessing from headless snapshots alone.

---

## 2026-04-13 — Boot detection is now a reusable headless workflow, not just a query

**Type:** milestone
**Trigger:** Once `boot.detected` and `screen.text.lines` were queryable, the next gap was obvious: host tooling still had to hand-roll frame polling to use them. That was enough for experimentation, but not enough for scripting, CLI automation, or ROM-backed workflow tests.
**Result:** the shared headless surface now has a first real boot-wait primitive, and Spectrum uses it for an honest keyboard-at-prompt system test:
1. Added `HeadlessSession::wait_for_boot(max_frames)` in `emu198x-shell`. It polls the generic `boot.detected` query once per native frame, returns a structured result (`frames`, `reached`, `reason`, `row`) on success, and fails with a typed timeout that carries the last `boot.reason`.
2. Added `wait_for_boot` as a shared JSON script step with a matching structured observation, so automation can block on boot semantically instead of hard-coding frame counts.
3. Added `--wait-for-boot N` to `emu198x-script-spectrum`, layered on the same shared helper. The runner now reports that wait in its JSON observations and fails explicitly when boot does not become visible within the requested frame budget.
4. Added shell and script tests around the new helper using a dummy query provider, plus a runner test proving zero-ROM boot waits time out instead of silently pretending success.
5. Added a new ignored ROM-backed Spectrum test that uses `wait_for_boot(250)`, then injects real key events at the BASIC prompt and verifies the decoded text screen changes from the prompt line to `NEW...` after pressing `A`. That is the first fresh-workspace proof that boot wait, keyboard injection, ROM code, and decoded screen text all work together on one real machine path.
**Verification:** `cargo test -p emu198x-shell`, `cargo test -p emu198x-script-spectrum`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should all pass. The new ROM-backed prompt test also passes locally with `cargo test -p runtime-sinclair-zx-spectrum spectrum_boot_wait_and_prompt_input_change_decoded_text -- --ignored --nocapture` when the 48K ROM is present.
**Next dependency:** the next high-value Spectrum system check is the same pattern applied to tape: boot, optionally wait for boot, start tape transport, and verify one concrete loaded-software path instead of stopping at ROM prompt interaction.

---

## 2026-04-13 — Spectrum boot detection is now queryable through the shared headless surface

**Type:** milestone
**Trigger:** After the machine-level timing checks landed, the next useful system-facing hook was not more synthetic tracing. It was a real boot-detected state that scripting, MCP, and future autoloading can consume directly instead of hard-coding frame counts or relying on screenshots.
**Result:** the Spectrum runtime now exposes generic machine-semantic boot and screen-text queries above the shared shell surface:
1. Added `screen.text.rows`, `screen.text.cols`, and `screen.text.lines` to the Spectrum query provider. For the 48K machine today these are derived by decoding the bitmap screen against the resident ROM font at `$3D00`, which is accurate for ROM text screens such as the boot banner.
   The ROM copyright glyph is normalized to Unicode `©` so the decoded line remains one text cell wide.
2. Added `boot.detected`, `boot.reason`, and `boot.row` on top of that decoded text surface. The current 48K boot detector reports success when the ROM copyright banner `(C) 1982 Sinclair Research Ltd` is visible on the decoded text screen.
3. Kept the implementation in the family runtime rather than the shared shell contract. The shell still owns generic query transport, while the Spectrum runtime owns the machine-specific meaning of “booted” and “text screen”.
4. Added synthetic unit tests for the bitmap-to-text decoder and boot-status parser, plus a new ignored ROM-backed runtime test that boots the real 48K ROM for 200 frames and proves that `boot.detected` becomes `true` with the expected reason and decoded text row.
5. Updated the scripting, MCP, and observability docs so the current repo stops describing `boot_detected` and `get_screen_text` as only future dedicated tools. Spectrum can now answer those concepts today through `query`.
**Verification:** `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` should all pass. The ROM-backed proof also passes locally with `cargo test -p runtime-sinclair-zx-spectrum spectrum_query_provider_detects_booted_48k_rom -- --ignored --nocapture` when the 48K ROM is present at `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`.
**Next dependency:** the next useful step is to let host workflows consume this directly: a simple boot-wait helper in the Spectrum runner or shared session layer, followed by the same query contract being adopted by the next runnable family.

---

## 2026-04-13 — Spectrum 48K machine loop now has normal-CI contention proofs

**Type:** milestone
**Trigger:** After the Z80 branch/contention fix was verified against FUSE, Tom Harte, `zexdoc`, and `zexall`, the remaining risk moved back up one level: we still needed proof that the fresh Spectrum 48K machine loop was actually exposing those bus patterns through the ULA-driven clocking model, not just inside the CPU in isolation.
**Result:** `machine-sinclair-zx-spectrum-48k` now has deterministic timing and trace coverage at machine level:
1. Added exact stepping helpers on the concrete 48K machine for half-cycles, T-states, and current frame-local T-state position. This stays below the shared runtime contract, but gives the machine crate a reusable deterministic timing surface for verification work.
2. Added Spectrum machine-loop trace helpers in the test module that record the real bus state seen after each CPU half-cycle under the ULA-driven outer loop.
3. Added a contention integration test proving that active-display fetches from contended RAM insert real CPU-clock gaps, while the same fetches from uncontended RAM do not.
4. Added machine-level regression tests for not-taken `DJNZ` and not-taken `JR cc` showing the fresh Spectrum loop now exposes the correct fallthrough behaviour:
   - a contended `PC` cycle with `MREQ` active and no read strobe
   - no displacement-byte memory read on the not-taken path
5. Exposed `spectrum.machine.tstate_in_frame` through the Spectrum query provider so headless scripts and future tooling can observe the machine timing state directly instead of inferring it from half-cycles.
**Verification:** `cargo test -p machine-sinclair-zx-spectrum-48k`, `cargo test -p runtime-sinclair-zx-spectrum`, `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass.
**Next dependency:** the next useful step is a stronger ROM- or software-driven system check that uses this verified timing surface to validate real Spectrum execution under display contention, rather than only synthetic instruction traces.

---

## 2026-04-12 — FUSE exact-trace verification is now live, and relative-branch contention is corrected

**Type:** milestone
**Trigger:** The first FUSE harness established final-state compatibility, but the exact event list still diverged because the fresh-workspace Z80 was not yet modeling every control-flow contention path the way FUSE records them. The decisive mismatch was `DJNZ` not taken: we were reading the displacement byte, while FUSE correctly showed a contended `PC` cycle without a read strobe.
**Result:** the fresh-workspace Z80 now has a stronger timing model and the FUSE harness now checks the whole instruction trace instead of only the end state:
1. Added exact event capture in `crates/zilog-z80/src/z80_fuse_tests.rs` for `MR`, `MW`, `MC`, `PR`, `PW`, and `PC`, including internal contention and port-timing phases.
2. Kept FUSE-specific address-selection logic in the harness instead of teaching production code FUSE-only heuristics. That preserves the chip model boundary while still comparing against the full reference trace.
3. Fixed a real Z80 timing bug in the core: not-taken `JR cc,e` and `DJNZ e` now use a contended `PC` cycle without a read strobe, instead of incorrectly reading the displacement byte.
4. Added an explicit `ContendPc` M-step and corresponding Z80 phase so the machine-visible bus behaviour matches the reference timing instead of faking the cycle as generic internal delay.
5. Re-ran the full local verification stack after the fix:
   - **FUSE:** `1,350 / 1,356` exact, `6` accepted disagreements, `0` unexpected, now on full event trace plus final state
   - **Tom Harte:** `1,604,000 / 1,604,000`
   - **ZEXDOC:** `67 / 67` checkpoints, `0` errors
   - **ZEXALL:** `67 / 67` checkpoints, `0` errors
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexdoc-after-branch cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexall-after-branch cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture`, and `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` all pass.
**Next dependency:** the CPU-side reference loop is now strong enough that the next high-value work is back at machine level: use the verified branch/contention behaviour under real Spectrum software and keep pulling timing bugs out of full-machine execution rather than synthetic CPU traces alone.

---

## 2026-04-12 — Fresh-workspace FUSE Z80 compatibility harness is established

**Type:** milestone
**Trigger:** After Tom Harte, `zexdoc`, and `zexall` were all passing in the fresh workspace, the next missing external verification pass was FUSE. The older repo claimed a five-failure FUSE result, but there was no current harness in this workspace and no reason to trust that old count without rerunning it here.
**Result:** the fresh workspace now has a local FUSE Z80 harness in `crates/zilog-z80/src/z80_fuse_tests.rs`:
1. Added a parser for the local FUSE `tests.in` and `tests.expected` fixture files, including register state, final T-state counts, expected memory deltas, and the event list for future use.
2. Added a chip-level runner that initializes the FUSE DEADBEEF memory background, applies fixture memory overlays, runs the half-cycle Z80 until the real post-instruction boundary, and compares final registers, memory, and T-state totals.
3. Established the current fresh-workspace FUSE baseline: **1,350 / 1,356 exact matches, 6 accepted disagreements, 0 unexpected**.
4. Made the six accepted disagreements explicit in the harness so any new FUSE drift or changed mismatch pattern fails the test immediately instead of hiding behind a generic percentage.
5. Corrected the stale repo narrative: the fresh workspace does not currently show the old "five failures" story. It shows six named disagreements, including an additional `INDR` `WZ` difference.
**Verification:** `cargo test -p zilog-z80 run_fuse_z80_reference_suite -- --ignored --nocapture` passes with `1,350 / 1,356 exact, 6 accepted disagreements, 0 unexpected`. `cargo clippy -p zilog-z80 --tests -- -D warnings` passes.
**Next dependency:** if we need FUSE-level event-trace comparison rather than final-state compatibility, the remaining work is not parser or fixture setup. It is trace instrumentation for internal `MC` / `PC` timing phases that are not fully visible on the public pin surface.

---

## 2026-04-12 — ZEX snapshots, cached resume, and full suite reruns are established

**Type:** milestone
**Trigger:** After adding checkpoint-targeted reruns, the remaining problem was practicality. Late checkpoint reruns still replayed the suite from reset, and the first full fresh-workspace `zexdoc` release run exposed two harness edge cases at real suite completion that the shorter tests had not covered.
**Result:** `crates/zilog-z80/tests/zex_tests.rs` now supports practical local ZEX iteration and has been proven against full suite runs:
1. Added a local snapshot format for the ZEX harness under `target/zex-snapshots` (or `EMU198X_ZEX_SNAPSHOT_DIR`) that stores the Z80 state, 64K CP/M memory image, completed checkpoint list, and cycle count.
2. Targeted checkpoint runs now resume from the highest cached checkpoint below the requested target instead of always restarting from reset. Full-suite runs also resume from the highest cached checkpoint when available.
3. Added fast harness tests covering snapshot round-trips, highest-checkpoint selection, completion-line handling, and extra summary output after the final checkpoint.
4. Fixed the two harness bugs discovered by real end-to-end runs:
   - `Tests complete` must count as completion even when it does not contain `OK`.
   - extra post-checkpoint summary output after checkpoint 67 must not be treated as a parser error.
5. Re-ran both exerciser suites end-to-end in release mode in the fresh workspace, and both now pass:
   - `zexdoc`: 67 checkpoints, 67 OK, 0 ERROR
   - `zexall`: 67 checkpoints, 67 OK, 0 ERROR
**Verification:** `cargo test -p zilog-z80 --test zex_tests`, `cargo clippy -p zilog-z80 --test zex_tests -- -D warnings`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-resume-proof EMU198X_ZEX_CHECKPOINT=1 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-resume-proof EMU198X_ZEX_CHECKPOINT=2 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture`, `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zex-release-full cargo test --release -p zilog-z80 --test zex_tests run_zexdoc -- --ignored --nocapture`, and `EMU198X_ZEX_SNAPSHOT_DIR=/tmp/emu198x-zexall-release-full cargo test --release -p zilog-z80 --test zex_tests run_zexall -- --ignored --nocapture` all pass.
**Performance note:** The release build matters here. The resumed checkpoint-2 `zexdoc` run took about `129s` in debug and `17.30s` in release from the same cached checkpoint.
**Next dependency:** FUSE is now the next external Z80 verification pass worth re-establishing in the fresh workspace, using the same “reference, not oracle” adjudication rule against Tom Harte and the now-passing ZEX suites.

---

## 2026-04-12 — ZEX harness now supports checkpoint-targeted reruns

**Type:** milestone
**Trigger:** After wiring the local ZEX binaries back into the fresh workspace, the remaining weakness in the harness was failure granularity. A failing `zexdoc` or `zexall` run still only told us that some point in a long exerciser program had gone wrong, not which labelled block had failed.
**Result:** `crates/zilog-z80/tests/zex_tests.rs` now treats the exerciser's own progress output as ordered checkpoints instead of raw console text:
1. Added the canonical 67 ZEX block labels as an explicit ordered checkpoint list, sourced from the local archived ZEX source files but now kept in-repo so the harness does not depend on those external source trees at runtime.
2. Reworked the CP/M console capture to preserve line structure from BDOS output, parse `OK` / `ERROR` status at line completion time, and record per-checkpoint metadata including index, label, and cycle count.
3. Kept the existing full-suite ignored tests for `run_zexdoc` and `run_zexall`, but added targeted ignored tests `run_zexdoc_checkpoint` and `run_zexall_checkpoint` driven by `EMU198X_ZEX_CHECKPOINT`, so a specific labelled block can be rerun intentionally.
4. Added fast parser-level tests so ordinary `cargo test` now verifies the checkpoint parser without needing local ZEX binaries or long exerciser runs.
**Verification:** `cargo test -p zilog-z80 --test zex_tests` passes. `cargo clippy -p zilog-z80 --test zex_tests -- -D warnings` passes. `EMU198X_ZEX_CHECKPOINT=1 cargo test -p zilog-z80 --test zex_tests run_zexdoc_checkpoint -- --ignored --nocapture` and the equivalent `run_zexall_checkpoint` both pass locally, each stopping cleanly after checkpoint 1 at `4,520,939,783` half-cycles and roughly `236s`.
**Next dependency:** checkpoint targeting improves diagnosis, but it does not make late-block reruns cheap because each targeted run still replays the prefix from reset. If we want practical routine use beyond early checkpoints, the next real improvement is save-state or resume support between checkpoints.

---

## 2026-04-12 — Z80 local verification corpora are wired back in; Tom Harte rerun passes cleanly

**Type:** milestone
**Trigger:** After the instruction-level integration coverage work, the next useful step was to stop treating `zexdoc`, `zexall`, and the Tom Harte corpus as aspirational references and make the fresh workspace actually discover and run the local verification assets that already exist on disk.
**Result:** the Z80 verification harnesses now use explicit local-corpus discovery instead of brittle hard-coded paths:
1. Added shared test-support lookup in `crates/zilog-z80/tests/support/mod.rs` for the Tom Harte Z80 corpus, ZEX binaries, and future FUSE fixtures. The harnesses now respect explicit environment variables first and then fall back to known local archive roots, including `~/Projects/Emu198x-Unclean/Reference/test-suites/...`.
2. Updated `single_step_tests.rs` to use that shared lookup path. The full Tom Harte run was then executed against the local `processor-tests/z80/v1` corpus and passed completely: **1,604,000 / 1,604,000 cases passing, 0 failed opcodes**.
3. Updated `zex_tests.rs` to discover local `zexdoc.com` / `zexall.com`, treat BDOS function 9 output as line-level progress rather than raw character spam, stop duplicating each BDOS call four times, and honor the exerciser's own `"complete"` message as the intended completion boundary instead of relying only on a final `HALT`.
4. Added an explicit reference-adjudication note to `wiki/concepts/test-methodology.md`: Tom Harte remains the primary per-instruction oracle, ZEX remains the program-level CPU regression suite, and FUSE stays a strong secondary reference for Spectrum-visible timing and bus behavior. Disagreements are to be recorded and resolved, not papered over.
**Verification:** `cargo test -p zilog-z80 --test single_step_tests run_opcode_00 -- --ignored --nocapture` passes against the local corpus (`1000/1000`). `cargo test -p zilog-z80 --test single_step_tests run_all -- --ignored --nocapture` passes with `1,604,000 / 1,604,000` cases. The improved `zexdoc` harness was exercised far enough to confirm correct local binary discovery and sane block-by-block progress reporting, but a full fresh-workspace ZEX rerun was not completed in this session.
**Next dependency:** if we want routine ZEX use rather than occasional long manual runs, the worthwhile next step is the per-block stop/resume or snapshot instrumentation Steve mentioned earlier, so a failing exerciser block can be isolated without replaying the entire program from the beginning.

---

## 2026-04-12 — Z80 ED edge cases and repeat variants narrow further

**Type:** milestone
**Trigger:** After the previous ED-prefixed pass, the remaining execute gaps were no longer broad instruction families. What stayed thin were the edge forms and line-distinct variants that matter to compatibility work: refresh-register transfers, undocumented `IN` / `OUT` register forms, the undocumented `IM 0` opcode alias, and the repeat or non-repeat variants whose implementation paths are separate in `execute.rs`.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers another focused ED slice through real instruction streams:
1. Added direct coverage for `LD R,A` plus `LD A,R`, including the refresh-counter interaction across prefixed fetches and the resulting flag state from the loaded `R` value.
2. Added explicit coverage for the undocumented ED forms `IM 0` via `ED 4E`, `IN F,(C)` as flags-only input, and `OUT (C),0` as zero-valued output.
3. Added the remaining distinct block-opcode variants that were still separate execute arms: `LDDR`, `CPD`, `INIR`, and `OTIR`.
4. The tests continue to assert externally meaningful behavior rather than internal helper details: actual emitted I/O writes, preserved registers on flags-only input, `HL` / `DE` directionality, repeat termination when `B` or `BC` reaches zero, and refresh-register effects that real machine code can observe indirectly.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `74.17%` line coverage to `78.07%`, total workspace region coverage improved from `78.36%` to `79.00%`, and total workspace line coverage improved from `81.16%` to `81.80%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next Z80-side work should stop being “cover every obvious ED arm” and shift toward the remaining genuinely thin machine-relevant behavior, likely interrupt sequencing, refresh-visible quirks, and any compatibility failures that show up once fuller machine software is driving the core.

---

## 2026-04-12 — Z80 ED-prefixed execute coverage expands through block, port, and 16-bit paths

**Type:** milestone
**Trigger:** After the previous execute-path passes, the biggest remaining holes in the Z80 core had shifted into the ED-prefixed space: interrupt-mode control, stack return paths, `IN` / `OUT` register forms, 16-bit ED arithmetic and indirect loads, nibble-rotate memory operations, and the backward or repeating variants of the block instructions.
**Result:** `crates/zilog-z80/tests/integration.rs` now drives another substantial ED-prefixed slice through real instruction streams:
1. Added direct integration coverage for `LD A,I`, `RETN`, `IM 1`, `IM 2`, `IM 0`, `IN r,(C)`, `OUT (C),r`, `ADC HL,rr`, `SBC HL,rr`, `LD (nn),rr`, `LD rr,(nn)`, `RLD`, `RRD`, and `LDD`.
2. Added backward and repeat-path coverage for the block families that were still thin after the earlier pass: `CPDR`, `IND`, and `OTDR`.
3. The new tests continue to verify machine-facing outcomes rather than internal helper state: stack-pop return addresses, restored interrupt flip-flops, `WZ` side effects, actual I/O bus writes, backward address movement, repeat termination when `B` reaches zero, and the flag behavior that real software depends on.
4. While landing the new `IN r,(C)` coverage, one test assumption had to be corrected: the parity flag for input value `0x81` is set, not clear, because the byte has even parity. The test now asserts the real flag result instead of the mistaken one.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `57.48%` line coverage to `74.17%`, `zilog-z80/src/alu.rs` improved from `68.63%` to `69.02%`, total workspace region coverage improved from `75.48%` to `78.36%`, and total workspace line coverage improved from `78.32%` to `81.16%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next worthwhile Z80 pass is to keep narrowing the remaining ED-prefixed gaps, especially the refresh-register transfer path (`LD R,A`, `LD A,R`) and any still-thin repeat or interrupt-control behavior that only shows up under real machine software.

---

## 2026-04-12 — Z80 direct transfer, rotate, flag, exchange, and port paths expand

**Type:** milestone
**Trigger:** After the previous execute-path pass, the remaining obvious holes in the Z80 core were no longer mostly control-flow branches. The thinnest areas had shifted to direct memory-transfer instructions, 16-bit pair arithmetic, rotate and flag-manipulation opcodes, alternate-register exchanges, and the single-byte port-I/O path.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers another substantial slice of the execute engine through real instruction streams:
1. Added direct bus-facing coverage for `LD A,(BC)`, `LD A,(DE)`, `LD (BC),A`, `LD (DE),A`, `INC (HL)`, `DEC (HL)`, `INC rr`, `DEC rr`, `ADD HL,rr`, `RLCA`, `RRCA`, `RLA`, `RRA`, `DAA`, `CPL`, `SCF`, `CCF`, `EX AF,AF'`, `EXX`, `IN A,(n)`, and `OUT (n),A`.
2. Added a dedicated I/O-write trace helper in the integration harness so single-byte port output is asserted at the transaction level instead of being inferred indirectly from internal state.
3. The new tests deliberately assert externally meaningful outcomes: memory bytes, register-pair values, carry/half-carry/sign behavior, alternate-register swaps, `WZ` updates where the core models them, and actual emitted I/O writes on the bus.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `49.76%` line coverage to `57.48%`, `zilog-z80/src/alu.rs` improved from `49.02%` to `68.63%`, total workspace region coverage improved from `72.08%` to `75.48%`, and total workspace line coverage improved from `75.29%` to `78.32%`.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next worthwhile Z80 pass is to keep driving down the remaining execute gaps in ED-prefixed and block/port behavior, plus any still-thin unprefixed instructions whose timing or side effects matter to real machine software.

---

## 2026-04-12 — Z80 execute-path integration coverage expands

**Type:** milestone
**Trigger:** After landing workspace coverage reporting, the next sensible use of that data was not to chase percentages blindly but to target real weak points in core behavior. The Z80 execute path was an obvious candidate: important control-flow and memory-transfer branches were present, but several of them were not being exercised directly by integration tests.
**Result:** `crates/zilog-z80/tests/integration.rs` now covers a materially wider slice of unprefixed control-flow and data-movement behavior:
1. Added direct integration coverage for `JP cc,nn` taken and not taken paths, `JP (HL)`, `CALL cc,nn` taken and not taken paths, `RET cc` taken and not taken paths, `DJNZ` taken and not taken paths, `RST 38h`, `EX (SP),HL`, `LD A,(nn)` / `LD (nn),A`, and `LD HL,(nn)` / `LD (nn),HL`.
2. These tests are machine-facing rather than isolated ALU assertions: they execute real instruction streams through the half-cycle core, verify resulting register and memory state, and exercise the walker's staged read/write/push/pop flow through the normal bus-facing integration harness.
3. While adding the `DJNZ` tests, one assumption in the new test code turned out to be wrong: the core's reset A state is not zero. The control-flow path itself was correct; the test was fixed to assert the branch outcome directly (`PC`, `B`, `HALT`) instead of assuming a reset accumulator value.
**Coverage note:** On the current local coverage run, `zilog-z80/src/execute.rs` improved from `40.90%` line coverage to `49.76%`, and total workspace line coverage improved from `73.59%` to `75.29%`. That is useful as a sanity signal, but the real gain here is the direct verification of previously untested execute branches.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p zilog-z80 --test integration`, `cargo test --workspace`, and `./scripts/coverage.sh` all pass.
**Next dependency:** the next high-leverage CPU-side increment is to keep working through the execute path with source-backed tests for remaining unprefixed and ED-prefixed instruction families, especially where line coverage is still low in `execute.rs`, `alu.rs`, and the block/IO sequences.

---

## 2026-04-12 — Coverage workflow and local reporting path land

**Type:** milestone
**Trigger:** The workspace had strong fast-test discipline and strict CI, but it still lacked one quantitative signal for how much of the current Rust surface was actually exercised. That made it harder to spot shallow wrappers, newly added untested code paths, and where the verification audit should focus first.
**Result:** Coverage reporting now exists as a first-class repo workflow:
1. `rust-toolchain.toml` now includes `llvm-tools-preview`, so the local toolchain can support source-based coverage without a separate manual component install.
2. New local entry point `scripts/coverage.sh` runs `cargo llvm-cov` for the whole workspace and writes four durable outputs under `target/llvm-cov/`: text summary, JSON summary, LCOV export, and HTML report.
3. New GitHub Actions workflow `.github/workflows/coverage.yml` runs that same script on pushes, pull requests, and manual dispatch. It publishes the `TOTAL` coverage line in the GitHub job summary and uploads both summary artifacts and the HTML report for inspection.
4. `docs/testing-policy.md` now records the intended use of coverage in this project: a directional audit signal, not a substitute for spec-driven testing or the verification ladder.
**Policy note:** This intentionally does not turn coverage percentage into the primary gate. The repo still treats reference-backed behavior and timing tests as the real bar. Coverage is there to show where the test surface is thin, not to certify cycle accuracy by itself.
**Verification:** The new coverage path was exercised locally with `./scripts/coverage.sh`, alongside the normal `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` gates.
**Next dependency:** the next useful step is a crate-by-crate coverage audit against the testing policy, especially for thin runtime and runner crates where percentages can now be compared against the actual verification matrix.

---

## 2026-04-12 — Spectrum family query namespace lands without widening `MachineCore`

**Type:** milestone
**Trigger:** The shared shell query surface was useful, but it only exposed session-owned state. The next gap was family-specific observability. That needed to land without turning `MachineCore` into a debugger or chip-inspection dumping ground.
**Result:** The shell now supports family-owned query namespaces through a separate `SessionQueryProvider` hook:
1. `emu198x-shell` now distinguishes between shared session queries and optional machine-family query providers. `HeadlessSession` can be created with a provider, and it merges provider-owned paths into `query_paths()` while falling back to provider-owned `query()` resolution only when a path is not part of the shared shell surface.
2. `runtime-sinclair-zx-spectrum` now ships `SpectrumSessionQueryProvider`, which owns the initial `spectrum.*` namespace: board issue, current half-cycle within the frame, keyboard matrix rows, and tape loaded/playing state.
3. `emu198x-script-spectrum` now boots its session with that provider, so shared JSON scripts can resolve both generic shell paths and Spectrum family paths through the same `query` / `query_paths` actions.
**Boundary note:** This was kept intentionally out of `MachineCore`. The runtime opts into family observability explicitly, and the shell still owns only the generic session model. That keeps chip- and family-specific inspection narrow and composable instead of making it part of the mandatory runtime contract for every machine.
**Documentation note:** `docs/features/scripting.md` now records the current Spectrum-owned `spectrum.*` paths in addition to the shared shell query paths.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes a shell test for provider-backed query extension, runtime tests for Spectrum query-path discovery and value resolution, and a Spectrum runner test that executes a script querying `spectrum.machine.issue`.
**Next dependency:** the next honest step is to decide how far family observability should go before we start needing explicit debugger namespaces, memory views, or trace/capture query surfaces. The current structure supports that growth, but it should stay deliberately narrow unless a concrete workflow needs more.

---

## 2026-04-12 — Shared session query surface and script observations land

**Type:** milestone
**Trigger:** The shell layer could already boot machines, load media, run frames, save captures, and execute JSON scripts, but scripts still only drove side effects. There was no shared way to ask the live session what it knew or to get structured results back from the script path itself.
**Result:** `emu198x-shell` now owns the first reusable observability surface above one live machine runtime:
1. New `query` module defines stable generic session paths, typed `QueryResult` / `QueryPathsResult` responses, and path resolution for current shell-owned state such as session time, profile metadata, capture availability, and the most recent run result.
2. `HeadlessSession` now tracks `last_run_result` and exposes `query()` plus `query_paths()`, so host-side tools can inspect live state without downcasting into family runtimes.
3. `HeadlessScript` and `ScriptStep` now support `query` and `query_paths` actions, and they return structured `ScriptObservation` values for `run_frames`, `query`, and `query_paths` instead of acting as pure fire-and-forget control flow.
4. `emu198x-script-spectrum` now emits one JSON report on stdout when `--script PATH` is used. That report includes structured observations plus final machine state (`time`, `tape_loaded`, `tape_playing`), which gives automation and future MCP-style hosts a real machine-readable result boundary.
**Documentation note:** `docs/features/scripting.md` now describes the current fresh-workspace contract instead of the older JSON-RPC-style scripting proposal. It reflects the real `action`-based step format, current shared actions, the implemented generic query paths, and the current JSON report shape.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for query-path filtering, run-state query resolution, session query access, script query observations, and a Spectrum runner test that executes a shared script and inspects the structured observation report.
**Next dependency:** the next useful increment is to widen observability carefully, likely with family-owned query namespaces above the same shell surface rather than by smuggling debugger or chip-inspection policy into `MachineCore`.

---

## 2026-04-12 — Shared headless session and JSON script runner land

**Type:** milestone
**Trigger:** The shell surface could already boot machines, load media, control transport, capture PNG/WAV, and save snapshots, but those operations were still being composed ad hoc inside the Spectrum CLI. The next gap was the reusable host-side workflow layer itself.
**Result:** `emu198x-shell` now owns that workflow layer:
1. `MachineCore` gained a `time()` accessor so host-side code can reason about authoritative machine progress without downcasting to family runtimes.
2. New `HeadlessSession` wraps one live machine runtime together with queued input events, frame capture, audio capture, and native-frame stepping. It owns the reusable operations a headless runner actually needs: prepare media/commands, run frames, save screenshots, save audio, save and restore snapshots, and queue host input.
3. New `HeadlessScript` / `ScriptStep` in `emu198x-shell` parse and execute shared JSON session scripts. The initial generic step set covers media loading, media transport, queued input events, frame execution, snapshot load/save, PNG screenshot export, and WAV audio export.
4. `emu198x-script-spectrum` now runs through that shared session layer for both direct CLI flags and `--script PATH`, instead of composing its own one-off host loop.
**Documentation note:** `docs/features/scripting.md` no longer claims that all four anchor families already have fresh-workspace script and MCP runners. Its top-level note now reflects the current truth: shared shell support exists, and Spectrum is the implemented runner on that path today.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for session stepping, queued input delivery, file-writing helpers, JSON script parsing/execution, and an end-to-end Spectrum runner test that executes a shared JSON script file.
**Next dependency:** the next useful increment is to keep pushing host policy into the shell layer by adding a structured result/query surface on top of the same session model, so future script and MCP paths can share more than just control flow.

---

## 2026-04-12 — Shared PNG/WAV capture lands on the shell surface

**Type:** milestone
**Trigger:** The headless path could boot, load media, control tape transport, and save snapshots, but it still had no shared way to turn emitted frame/audio packets into durable artifacts. Capture remained only a documented intention.
**Result:** `emu198x-shell` now owns the first reusable capture layer:
1. New `capture` module stores the latest emitted frame or a whole audio stream through `LatestFrameCapture` and `AudioCapture`, both implementing the shared `FrameSink` / `AudioSink` traits directly.
2. The shell can now convert raw machine output into real artifacts without family-specific code: indexed or RGBA frames encode to PNG, and captured audio encodes to 16-bit PCM WAV.
3. `emu198x-script-spectrum` now exposes that shared path through `--screenshot PATH` and `--audio-capture PATH`. The runner still stays thin: it just selects the capture sinks, runs frames, and writes the encoded bytes returned by the shell helpers.
**Boundary note:** Capture remains strictly above the runtime boundary. The Spectrum runtime still only emits raw indexed video and float audio packets; PNG and WAV are host-side concerns owned by the shell layer.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for indexed-frame PNG output and WAV output plus runner tests that boot a zero ROM, emit one frame, and write both artifact types.
**Next dependency:** the next logical step is to use the same shared shell surface for scripted headless workflows, so capture, boot, media control, and later MCP methods all compose around one host-side session model instead of ad hoc CLI glue.

---

## 2026-04-12 — Shared firmware bootstrap and media transport control land

**Type:** milestone
**Trigger:** The first Spectrum headless runner worked, but it still owned too much host policy itself: firmware was a hard-coded `--rom` path interpreted directly by the binary, and tape start/stop bypassed the shared control surface via `Spectrum48kRuntime` methods.
**Result:** `emu198x-shell` now owns the first reusable host-side bootstrap/control layer:
1. New shared firmware types `FirmwareImage` and `FirmwareSet` validate declared firmware ids against `MachineProfile` requirements, catching missing, duplicate, and unknown firmware before family runtimes try to boot.
2. `MachineCore` now accepts shared `ControlCommand`s, with the first concrete command family being media transport (`start` / `stop` on a named slot).
3. New `boot_machine()` and `prepare_machine()` helpers formalize the thin-runner path: construct from firmware or a blank runtime for snapshot restore, then apply media inserts plus shared control commands.
4. `runtime-sinclair-zx-spectrum` now implements that contract directly: `Spectrum48kRuntime::from_firmware()` resolves the declared 48K ROM id, and tape playback is driven through shared media-transport commands on slot `tape-1`.
5. `emu198x-script-spectrum` is now a genuinely thin adapter. It still supports the Spectrum-friendly aliases (`--rom`, `--tape`, `--play-tape`), but its real path is shared and profile-driven: `--firmware ID=PATH`, `--media SLOT:KIND=PATH`, `--start-slot`, `--stop-slot`, snapshot load/save, then frame execution.
**Boundary note:** This is intentionally still host-side policy. The runtime validates firmware and honors transport commands, but it does not gain any filesystem or CLI knowledge, and the machine core still owns only hardware state and timing.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes shell tests for firmware validation and bootstrap/control helpers, runtime tests for declared-firmware boot plus tape transport commands, and script-runner tests for both generic flags and Spectrum compatibility aliases.
**Next dependency:** the next useful step is to keep extracting headless policy out of one binary by building capture/scripting entry points on the same shared shell surface rather than teaching each family runner its own bespoke workflow.

---

## 2026-04-12 — Spectrum runtime snapshots and headless runner land

**Type:** milestone
**Trigger:** The fresh-workspace Spectrum path had an honest machine loop, media parsing, and a `MachineCore` runtime, but there was still no durable state handoff and no small headless entry point that could supply firmware, load tapes, drive playback, and save/restore execution state.
**Result:** Two connected boundaries landed together:
1. `runtime-sinclair-zx-spectrum` now owns versioned runtime snapshot import/export. `Spectrum48kRuntime::snapshot()` serializes machine time plus validated 48K machine state into a postcard envelope, and `restore()` rejects wrong profile/version payloads before rebuilding the live machine.
2. New crate `emu198x-script-spectrum` provides the first headless family runner in the fresh workspace. It cold-boots from a ROM, optionally restores a snapshot, loads `tape-1` media from TAP/TZX bytes, explicitly starts tape playback, runs an exact frame count on the native Spectrum cadence, and can write a new runtime snapshot on exit.
**Design note:** The machine snapshot boundary is explicit rather than deriving `Serialize` directly on the whole machine. Large ROM/RAM arrays are flattened into `Spectrum48kSnapshot`, which keeps restore validation local to the machine crate and avoids pretending that every internal type is part of a stable wire format.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` all pass. New coverage includes runtime snapshot round-trip tests and runner tests for CLI parsing plus ROM boot to snapshot output.
**Next dependency:** the next honest step is to make the headless Spectrum path less ad hoc by formalizing firmware/tape control policy above the runtime boundary instead of leaving it embedded in one family-specific script binary.

---

## 2026-04-12 — Spectrum media parsers, runtime wrapper, and beeper audio land

**Type:** milestone
**Trigger:** The 48K machine had a real frame loop and tape progression, but there was still no honest shell-facing media path and no machine-emitted audio packet path.
**Result:** Three connected changes landed together:
1. New crates `format-sinclair-zx-spectrum-tap` and `format-sinclair-zx-spectrum-tzx` now parse the two baseline Spectrum tape formats into machine-usable structures/pulse streams.
2. `common-sinclair-zx-spectrum` gained `BeeperAudio`, and `machine-sinclair-zx-spectrum-48k` now models the beeper/EAR speaker path at T-state precision, emitting one mono audio frame alongside each video frame.
3. `runtime-sinclair-zx-spectrum` now includes `Spectrum48kRuntime`, the first fresh-workspace `MachineCore` implementation: it owns a real 48K profile, validates ROM bytes at construction, accepts `MediaSet` tape loads on slot `tape-1`, forwards host key events into the keyboard matrix, and emits indexed video plus mono audio packets through the shell sinks.
**Accuracy note:** Tape EAR no longer keeps driving `$FE` after the virtual deck stops. Tightening that behavior fixed an earlier overreach in the machine tests; once playback ends, bit 6 falls back to the ULA/tape-input boundary rather than a stale tape level.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. New coverage includes TAP/TZX parser tests, machine audio tests, and runtime tests for `MediaSet` loading plus frame/audio emission.
**Next dependency:** snapshot import/export on the new runtime boundary and then a real family product/runner layer that can supply firmware and drive tape control without smuggling policy into the machine core.

---

## 2026-04-12 — Spectrum tape progression lands; ROM-backed boot smoke test added

**Type:** milestone
**Trigger:** The 48K machine had a real ULA/Z80 frame loop but tape was still only a static EAR-line override. The next honest step was to make media advance on the real 3.5 MHz T-state cadence.
**Result:** `common-sinclair-zx-spectrum` now owns a shared pulse-driven `TapePlayer` plus standard ROM-speed block-to-pulse helpers. `machine-sinclair-zx-spectrum-48k` now wires that player into the live frame loop: the machine advances tape every T-state (`hc % 4 == 2`), exposes the current EAR level through `$FE`, keeps the external `TapeInput` override as a higher-priority boundary for non-player sources, and adds machine-local load/play/stop helpers for raw pulses and standard blocks.
**Boot-path note:** The 48K machine now also carries an ignored ROM-backed smoke test that loads `~/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom`, runs 200 frames, and asserts that the ROM has populated both pixel RAM and attribute RAM. This is intentionally a smoke test, not a claim of completeness.
**Quality note:** The imported tape player was tightened while porting: `play()` now resumes a partially consumed pulse instead of rewinding it, and zero-length pulses are consumed without risking an infinite loop.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. The workspace now includes shared tape unit tests, machine tests for T-state-driven tape progression, and an ignored ROM-backed boot test hook.
**Next dependency:** actual media format ingestion at the machine/runtime boundary (`.tap` / `.tzx`) and then the 48K beeper/EAR audio path, both driven from this same T-state cadence rather than from host-time shortcuts.

---

## 2026-04-12 — Spectrum 48K machine crate lands; firmware boundary gap noted

**Type:** milestone + design note
**Trigger:** First fresh-workspace machine-layer implementation for the Spectrum 48K.
**Result:** New crate `machine-sinclair-zx-spectrum-48k` owns the first honest machine-local state: 48K memory delegation, the 8 half-row keyboard matrix, shell `InputEvent::Key` translation, tape EAR input state, and board-issue-correct `$FE` read/write behaviour (Issue 2 vs Issue 3 bit-6 feedback, with tape override when connected).
**Source notes:** The matrix key encoding ports cleanly from the older runtime crate; the board-issue `$FE` behaviour ports from the old Ferranti ULA tests. The old bus loop and ULA timing code were deliberately *not* reused here.
**Design note for future sessions:** `emu198x-shell::MachineCore` still has media loading but no firmware-loading boundary. That means ROM-dependent machine crates should stay *below* the shell trait for now rather than faking firmware as media or inventing half-initialized constructors. Revisit the shell boundary after at least one real machine path proves what the firmware handoff actually needs to look like.

---

## 2026-04-12 — Z80 crate ported into the fresh workspace

**Type:** milestone
**Trigger:** The Spectrum path reached the point where another support crate would just defer the real dependency. The next honest move was the CPU.
**Result:** `zilog-z80` is now present in the fresh workspace as the real half-cycle, pin-level Z80 core from the fresh-start lineage: public bus pins (`addr`, `data`, `mreq`, `iorq`, `rd`, `wr`, `m1`, `rfsh`, `halt`), input pins (`data_in`, `wait`, `irq`, `nmi`), static M-step sequences, and the instruction walker/ALU/register file needed for real execution.
**Verification:** Workspace checks pass with the imported crate under the current repo lint bar: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. The port carries 19 unit tests and 31 integration tests locally, plus ignored Tom Harte and ZEX harnesses.
**Quality note:** The initial quick port temporarily allowed `clippy::unwrap_used` in the test harnesses. That was immediately removed and the harnesses were rewritten to use explicit control flow instead, so the crate now matches the repo policy cleanly rather than by exception.
**Next dependency:** the Ferranti 6C001E ULA wrapper and the first real 48K machine loop that wires ULA gating to the Z80 pins.

---

## 2026-04-12 — Ferranti ULA and first real 48K frame loop land

**Type:** milestone
**Trigger:** With the pin-level Z80 in place, the next honest step was to stop modeling `$FE` and contention in isolation and wire the real 48K video chip into the machine.
**Result:** Three linked changes landed together:
1. `common-sinclair-zx-spectrum` grew the shared ULA substrate: palette helpers, `FrameTiming`, the Spectrum `Ula` trait, and the shared `UlaEngine`.
2. New crate `ferranti-ula-6c001e` ports the 48K Ferranti wrapper, including board-issue-specific EAR feedback (`Issue2` MIC-or-EAR vs `Issue3` EAR-only).
3. `machine-sinclair-zx-spectrum-48k` now owns a real 48K frame loop: the Ferranti ULA ticks against the 48K memory map, gates the Z80 clock, feeds IRQ, performs bus reads/writes, exposes the rendered framebuffer, and uses the ULA's floating-bus behaviour for unattached odd-port reads.
**Quality note:** The temporary machine-local `$FE` latch model was retired from the machine path. Tape EAR override still lives at the machine boundary because that line is external to the ULA core; border/beeper/keyboard feedback now come from the actual ULA implementation.
**Verification:** `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` all pass. New local coverage includes palette tests, Ferranti board-issue tests, and a `run_frame()` smoke test for the integrated 48K machine.
**Next dependency:** honest media/tape progression and then ROM-backed boot-path tests against the new machine loop, rather than the former state-only machine shell.

---

## 2026-04-10 — Amiga boot screen debugging: root cause narrowed

**Type:** investigation
**Trigger:** Kickstart 1.3 shows white screen despite all OS inits completing.
**Key finding:** Compared chip RAM and register state against FS-UAE running the same Kickstart 1.3 with same 512K config. Chip RAM at $000-$600 is **byte-for-byte identical**. CPU instructions produce correct results. JMP table at $400+ matches exactly.

**Ruled out:** CPU instruction bugs, CPU speed (4× still white), copper corruption (COPJMP2 disabled still white), memory detection, init sequence (all residents run), chip RAM aliasing, autoconfig, byte-write merging, DMA contention, CIA init, TAS.

**Root cause:** Graphics.library never builds the display copper list. The COP2 display list that FS-UAE has at $10450 (WAIT→colors→DIWSTRT→2-plane bitplanes→END) does not exist anywhere in our chip RAM. COP2LC address also differs ($2408 ours vs $10450 FS-UAE).

**Top lead for next session:** The archive used **ECS Agnus/Denise wrappers** that provide BEAMCON0=$0020 (PAL) and other ECS registers. FS-UAE also returns BEAMCON0=$0020. Our pure OCS Agnus returns 0 for BEAMCON0. If graphics.library or the strap task reads BEAMCON0 to determine PAL mode and gets 0, it may skip display creation.

**Fixes applied this session:** Byte-write merging for custom registers, chip RAM DMA bus contention, CIA-A external_a=$EB, CPU reset_to(), autoconfig bus float, VPOSR v9/v10 bits, Gary slow_ram config, 13→19 pin-level 68000 tests.

**Workspace:** 730 tests, 0 failures, 62 crates. 12 commits this session.

---

## 2026-04-10 — Amiga Phase 8: runtime + CLI + screenshot

**Type:** milestone
**Trigger:** Phases 1-7 complete (all chips + machine + peripherals). Needed visible output.
**Result:** Two crates:
1. `runtime-commodore-amiga` (4 tests) — RGBA framebuffer conversion from Denise's ARGB32 raster buffer, cropped to 724×568 visible display area.
2. `emu198x-script-amiga` — headless CLI: `emu198x-script-amiga kick.rom --frames N --screenshot out.png [--adf disk.adf]`
**Kickstart 1.3 boot status:**
- CPU executes from ROM, clears overlay, sets up exception vectors ✓
- Keyboard power-up init ($FD/$FE handshake) completes ✓
- VERTB interrupt fires every frame, CPU handles via autovector ✓
- exec.library scheduler reached (STOP #$2000 idle loop) ✓
- DMA enabled (bitplane + copper + blitter + sprite) ✓
- Copper list at $2368 runs for 3 frames before being replaced ✓
- **Not yet working:** boot animation (hand/insert-disk screen). The graphics.library task that maintains the persistent copper list isn't setting COP1LC after exec init. This is a CPU/scheduler interaction issue — the 68000's instruction execution is correct (7 pin-level tests + 200 frames of successful Kickstart init), but the OS-level task scheduling needs further debugging. Same class of issue as the NES port where the machine wiring was the bottleneck, not the chip logic.
**Workspace totals:** 724 tests passing, 0 failing, 18 ignored (62 crates).

---

## 2026-04-10 — Amiga Phase 7: floppy + keyboard + ADF

**Type:** milestone
**Trigger:** Phase 6 (machine wiring) complete — peripherals needed for Kickstart to proceed past early init.
**Result:** Three crates ported from archive as clean lifts:
1. `format-commodore-amiga-adf` (139 lines, 6 tests) — ADF image parser, DD/HD support, sector read/write.
2. `peripheral-commodore-amiga-floppy` (480 lines + 492 MFM, 24 tests) — drive mechanism: head positioning, motor spin-up, disk change, MFM track encode/decode with Amiga odd/even bit-split format, sector write-back via DiskImage trait.
3. `peripheral-commodore-amiga-keyboard` (357 lines, 8 tests) — power-up init sequence ($FD/$FE with handshake), rotated keycode transmission, timeout/resend.
All three wired into `machine-commodore-amiga`:
- Keyboard ticks on E-clock, injects serial bytes into CIA-A SDR, handshake on CIA-A CRA bit 6 falling edge
- Floppy ticks on E-clock (motor spin-up), status feeds CIA-A PRA (DSKCHANGE/DSKPROT/DSKTRACK0/DSKRDY), control from CIA-B PRB (step/dir/side/sel/motor)
- Disk DMA now encodes from real floppy track data instead of dummy stream
**Workspace totals:** 720 tests passing, 0 failing, 18 ignored (60 crates).
**Next:** Phase 8 — runtime + headless CLI + PNG screenshot. Validation target: Kickstart 1.3 hand/insert-disk screen.

---

## 2026-04-10 — Amiga Phase 6: machine-commodore-amiga wiring

**Type:** milestone
**Trigger:** Continuation from phases 1-5 (all OCS chips ported). Phase 6 is the machine wiring — the "moment of truth" where the clock tree drives everything.
**Result:** `machine-commodore-amiga` crate landed with 16 tests (10 machine + 6 memory). Master-clock-driven tick loop implements the amiga-port-plan.md pseudocode exactly:
- CCK every 8 master clocks: Agnus beam advance + DMA slot allocation (bitplane, sprite, disk, copper, audio, blitter) + Denise pixel output + Paula audio DMA + audio downsampling
- CPU every 4 master clocks: 68000 State enum inspection for bus servicing via Gary address decode → chip RAM / Kickstart ROM / slow RAM / CIA-A / CIA-B / custom registers. Interrupt ack returns autovector. Paula IPL → CPU ipl pin routing.
- E-clock every 40 master clocks: CIA-A/CIA-B tick, CIA IRQ → Paula interrupt routing
- Full custom register read/write dispatch (Agnus/Denise/Paula/Copper), including BPLCON0/DDFSTRT/DDFSTOP/color register pipelining (2-CCK Agnus→Denise delay)
- Full synchronous blitter (area + line mode) ported from archive
- Disk DMA with WORDSYNC, sprite DMA phase state machine, bitplane DMA with vertical enable flip-flop and modulo application
- Serial port minimal model (TBE always initially set for Kickstart boot)
- run_frame() advances by one PAL frame, stereo audio with RC low-pass filter
**New crates:** `machine-commodore-amiga` (16 tests)
**Workspace totals:** 682 tests passing, 0 failing, 18 ignored (57 crates).
**Next:** Phase 7 (floppy + keyboard) for Kickstart to proceed past init, then Phase 8 (runtime + CLI + screenshot) for visible output. The validation target is booting Kickstart 1.3 to the hand/insert-disk screen.

---

## 2026-04-10 — Amiga phases 1-5: 68000 + all OCS chips ported

**Type:** milestone
**Trigger:** User chose to push through to Amiga after NES was complete.
**Result:** Six Amiga crates landed:
1. `motorola-68000` (14,167 lines) — pin-level bus conversion from archive's `M68kBus` trait. tick() reads `bus_status` and `ipl` pin fields. 7 pin-level tests (MOVEQ, MOVE, ADD, JSR/RTS, memory read/write, DBRA loop, supervisor mode). 68020+ synchronous bus ops stubbed.
2. `mos-cia-8520` (634 lines, 18 tests) — clean lift, Amiga CIA variant.
3. `commodore-gary` (687 lines, 37 tests) — clean lift, address decoder.
4. `commodore-agnus-ocs` (1,706 lines, 30 tests) — clean lift, beam + DMA + copper.
5. `commodore-denise-ocs` (2,319 lines, 18 tests) — clean lift, pixel pipeline.
6. `commodore-paula-8364` (1,394 lines, 8 tests) — clean lift, audio + interrupts.
**Workspace totals:** 666 tests passing, 0 failing, 18 ignored.
**Next:** Phase 6 — machine-commodore-amiga wiring (master clock → Agnus → DMA → CPU + Denise + Paula). This is the largest remaining piece (~6,600 lines in the archive). Start by reading the archive's `machine-commodore-amiga/src/lib.rs` tick loop and rewriting for pin-level CPU bus. Target: boot Kickstart 1.3.

---

## 2026-04-10 — APU ported + System trait + archive cleanup

**Type:** milestone
**Trigger:** Continuation after nestest + SMB screenshot. APU was the last missing chip; System trait was needed for shell integration.
**Result:** Three deliverables:
1. `ricoh-apu-2a03` crate — clean lift from archive, 21 tests pass unchanged. Wired into machine-nintendo-nes: ticks per CPU cycle, registers routed, IRQ OR'd into cpu.irq, DMC DMA bytes fed from mapper.
2. `runtime-nintendo-nes` System trait impl — family=Nes, model=nintendo-nes-ntsc, RGBA framebuffer, 48 kHz mono audio, controller input via inject_input, register read/write. 5 runtime tests.
3. Archive cleanup — `ricoh-apu-2a03` deleted from archive (commit `3d8c51d60b`). Remaining NES crates in archive: `format-nintendo-nes-ines` (47 mappers), `emu-nintendo-nes{,-wasm}` (frontend/WASM).
**New crates:** `ricoh-apu-2a03` (21 tests)
**Pages created:** `chips/ricoh-apu-2a03.md`
**Pages updated:** `index.md`, `decisions/archives-as-source.md` (NES source map + cleanup history)
**Workspace totals:** 455 tests passing, 0 failing, 18 ignored.

---

## 2026-04-10 — nestest: 8991/8991 instructions matched

**Type:** milestone
**Trigger:** Machine wiring landed — natural next step was running real NES code.
**Result:** nestest.nes (Kevin Horton's CPU instruction exerciser) passes with **8,991 / 8,991 instructions matching** the golden log. Register state (PC, A, X, Y, P, SP) compared at every instruction fetch. Test result codes: `$02` (official opcodes) = `0x00`, `$03` (unofficial opcodes) = `0x00` — all official and unofficial opcodes pass.
**New test file:** `crates/machine-nintendo-nes/tests/nestest.rs` — smoke test (first 100 instructions, runs on every `cargo test`), full suite (`#[ignore]`'d, ~0.01s in release).
**What this validates:** the complete NES chip stack working together under real code — tick loop timing, address space routing, PPU register bus, CPU instruction correctness, 2A03 BCD-disabled behaviour. This is the NES equivalent of the C64 boot-to-READY test.
**Workspace totals:** 429 tests passing, 0 failing, 18 ignored.

---

## 2026-04-10 — machine-nintendo-nes: NES machine wiring

**Type:** milestone
**Trigger:** Continuation after PPU port — all three NES chip crates were ready (2A03 CPU, 2C02 PPU, iNES parser + NROM mapper), so the machine wiring was the natural next step.
**Result:** `machine-nintendo-nes` crate landed with 12 tests. Master-clock tick loop implements the nes-clock-topology decision doc exactly: PPU every dot, CPU every 3rd dot, NMI/IRQ routed between ticks. OAMDMA stalls CPU for 514 cycles. Controller 1 serial shift register. Full NES address space (2 KiB RAM mirrored, PPU registers, APU stubs, mapper). `run_frame()` runs until pre-render → scanline 0 transition.
**New crates:** `machine-nintendo-nes` (12 tests)
**Pages created:** `systems/nintendo-nes.md`
**Pages updated:** `index.md` (NES system overview added)
**Workspace totals:** 428 tests passing, 0 failing, 17 ignored.
**Next:** nestest.nes validation (golden log comparison), then runtime + headless CLI.

---

## 2026-04-10 — Ricoh 2C02 PPU: dot-level rendering ported

**Type:** milestone
**Trigger:** Continuation of NES port after phase 1 (2A03 + iNES parser + clock topology). Steve confirmed that the machine wiring was the architectural issue, not the PPU's rendering logic itself — so the port was scoped as an interface rewrite rather than a logic rewrite.
**Result:** `ricoh-ppu-2c02` crate ported from archive with 20 tests. Internal rendering logic (background tile fetch pipeline, sprite evaluation with hardware overflow bug, pixel composition, loopy scroll registers, NMI timing, odd-frame skip) lifts intact. Interface changes: `tick()` takes `&mut dyn Mapper` instead of closures; `nmi` is a public active-high field instead of active-low with edge-detection helpers; A12 transitions call `mapper.notify_a12_rendering()` directly from inside `tick()` instead of deferring for the machine to poll.
**New crates:** `ricoh-ppu-2c02` (20 tests)
**Pages created:** `chips/ricoh-ppu-2c02.md`
**Pages updated:** `index.md` (PPU chip line added)
**Workspace totals:** 416 tests passing, 0 failing, 17 ignored.

---

## 2026-04-10 — NES phase 1: 2A03 variant, iNES parser, clock topology

**Type:** milestone
**Trigger:** Steve chose tight NES scope (Option B) — 2A03 CPU variant + Tom Harte validation, iNES/NES 2.0 parser with NROM only, clock topology decision doc. No PPU scaffolding this session.
**Result:** Three deliverables landed:

1. **2A03 CPU variant** — `M6502::new_2a03()` constructor sets `decimal_disabled: true`, gating the BCD paths in `alu_adc` and `alu_sbc`. Validated against the Tom Harte NES fixture (`nes6502/v1/`): **2 470 000 / 2 470 000 stable opcodes passing, zero regressions.** Same 9 unstable undocumented opcodes excluded. Bonus: `6b` (ARR #imm) passes 10 000/10 000 on NES because BCD disabled makes it deterministic.
2. **`format-nintendo-nes-ines` crate** — iNES 1.0 + NES 2.0 header parser, `Mapper` trait, NROM (mapper 0) implementation. 17 tests covering 16 KiB/32 KiB PRG, CHR ROM/RAM, mirroring modes, PRG RAM, battery flag, NES 2.0 12-bit mapper numbers, error cases. The other 47 mappers from the archive are deferred until the PPU crate is online.
3. **`wiki/decisions/nes-clock-topology.md`** — formal decision doc for the NES master-clock-driven tick loop (RULES.md item 1), PPU every dot, CPU every 3rd dot, pin contracts for PPU/CPU/Mapper, drift triggers, OAMDMA/DMC DMA stall shapes.

**New crates:** `format-nintendo-nes-ines` (17 tests)
**Pages created:** `decisions/nes-clock-topology.md`
**Pages updated:** `chips/mos-6502.md` (Variants section added, test coverage expanded to cover both suites), `index.md` (NES section added, crate + decision listed)
**Workspace totals post-session:** 396 tests passing, 0 failing, 17 ignored.

---

## 2026-04-09 — Tom Harte 6502 regression suite: 2.47M / 2.47M stable

**Type:** milestone
**Trigger:** Option B from the post-READY-screenshot planning — Tom Harte validation of the mos-6502 port against <https://github.com/SingleStepTests/65x02>. Fixture found on disk at `~/Projects/Emu198x-archive/test-data/65x02/6502/v1/` (1 GiB, 256 JSON files, 2.56 M test cases).
**Result:** **2 470 000 / 2 470 000 stable opcodes passing, zero regressions.** Every one of the 151 documented 6502 opcodes passes every Tom Harte test case (register state, memory state, cycle counts). 96 of the 105 undocumented opcodes also pass cleanly. The 9 that don't match (`6b`, `8b`, `93`, `9b`, `9c`, `9e`, `9f`, `ab`, `bb`) are exactly the famously-unstable opcodes whose behaviour varies between chip revisions and ambient temperature on real hardware — the port stubs them as `NopRead` per the archive's "Unstable undocumented" comment block, and the regression suite excludes them via a hardcoded allow-list.
**New test file:** `crates/mos-6502/tests/tom_harte.rs` (~430 lines) — pin-level test harness with fixture resolution (`MOS_6502_TEST_DATA` env var / known-good archive path / in-tree `test-data/`), 4 smoke tests that run on every `cargo test` (~150 ms for 40 000 cases), and the `#[ignore]`'d `run_all` regression suite (~6 s in release mode for 2.56 M cases).
**Pages updated:** `chips/mos-6502.md` (Test coverage section expanded, "Known gaps" section reshaped — Tom Harte gap closed, AbsX cycle-count quirk confirmed as not-a-bug, unstable undocumented opcodes documented as deliberate and accepted).
**What this validates:** the mos-6502 port is now validated to gold-standard level for every opcode that any real software uses. The pin-level pipelined tick model, every addressing mode, every flag update, every cycle count, BCD arithmetic in both ADC and SBC, indirect-Y with page cross, BRK/IRQ/NMI stack pushes, RMW three-write-phase cycles — all correct. The only remaining 6502 gap that matters is the unimplemented unstable undocumented opcodes, and those are consciously excluded.
**A small bonus finding:** the previously-flagged `tick_absolute` AbsX no-cross "1-extra cycle" concern was actually **not a bug** — the Tom Harte tests on `bd.json` (LDA abs,X) pass cleanly, proving the cycle count is correct. The concern came from a misreading of the archive's code during the port. Updated the wiki to reflect this.
**Runtime:** 6.75 seconds for 2.56 M test cases in release mode on this machine. Cheap enough to run on every PR that touches mos-6502 source.

---

## 2026-04-09 — C64 runtime + CLI: first visible READY. screenshot

**Type:** milestone
**Trigger:** Immediately after the machine-wiring boot test confirmed the KERNAL ran end-to-end in RAM, Steve picked Option A (runtime + frontend) from the next-step planning. The session built `runtime-commodore-c64` (wrapper + `System` trait impl + RGBA conversion + file loader), `emu198x-script-c64` (headless CLI with fast-path flags), and ran it against the real ROMs to produce a PNG of the booted BASIC prompt.
**Result:** **Rendered.** 120-frame boot run produced a 416×312 8-bit RGBA PNG at `/tmp/c64-screenshots/boot-120frames.png` showing the classic `**** COMMODORE 64 BASIC V2 ****` / `64K RAM SYSTEM  38911 BASIC BYTES FREE` / `READY.` banner, in the light-blue-on-blue C64 palette, with the cursor after `READY.`.
**Pages updated:** `systems/commodore-c64.md` (phase 4 section added), `index.md` (not yet — will follow in a doc pass when runtime-commodore-c64 and emu198x-script-c64 are public).
**New crates:** `runtime-commodore-c64` (12 tests), `emu198x-script-c64` (headless CLI, no tests — it's wrapper code over the runtime).
**What this validates:** the remaining unknowns after phase 3 — the `System` trait implementation, RGBA conversion from the VIC-II's `Vec<u32>` framebuffer, integration with `emu198x-shell::encode_png_to_file`, the CLI fast-path for headless captures. All clean, all first-try. The RGBA re-pack runs per frame (O(width × height) shift-and-mask on each pixel) and the resulting byte order matches the shell's `PixelFormat::Rgba8888` expectation (R G B A byte order, not the BGRA that a naive little-endian slice cast would have produced).
**Workspace totals post-session:** 372 tests passing, 0 failing, 15 ignored (the 15 are the boot-to-READY test, a Tom Harte fixture test awaiting vendoring, and a handful of long-running integration tests in other crates).

---

## 2026-04-09 — C64 machine wiring boots the KERNAL end-to-end

**Type:** milestone
**Trigger:** Steve located the C64 ROMs (`basic.rom`, `chargen.rom`, `kernal.rom`) in `Emu198x-archive-april2026/roms/c64/` and the `#[ignore]`'d boot-to-READY integration test in `machine-commodore-c64` was run against them.
**Result:** **Booted first try.** `Found READY. at frame 108, offset $00C8` — the KERNAL reached the BASIC `READY.` prompt at frame 108 (~2.16 s of emulated C64 time, matching real hardware's ~2.5 s cold-boot timing). Test runtime: ~2.35 s.
**Pages updated:** `systems/commodore-c64.md` (implementation-status section now records the validation milestone, boot-test section documents the known-good ROM location and command).
**What this validates:** every architectural decision from the chip wave + machine wiring — pin-level CPU bus (RULES.md item 6), the `VicMemory` trait, one-op-per-tick discipline, tick ordering, IRQ routing (VIC∪CIA1→irq, CIA2→nmi), RDY-only-gates-reads semantics, `$00`/`$01` port banking, the 6510 I/O-port routing at `$D000-$DFFF`, and the CIA1 keyboard scan hand-off through `pb_in`. ~2 million real KERNAL opcodes executed without an illegal-instruction trap, a stack overflow, or a stuck-on-BRK loop. The bad-line BA assertion stalled the CPU correctly without deadlocking. Memory banking put BASIC and KERNAL in the right places for the KERNAL's own boot-time reads.
**Known-good ROMs:** `~/Projects/Emu198x-archive-april2026/roms/c64/{basic,chargen,kernal}.rom` (8192/4096/8192 bytes).
**Why this is a big deal:** before this run, every chip had been tested in isolation with hand-written fixtures; the machine had been tested with stub ROMs containing sentinel byte patterns. No evidence existed that the chips *worked together* under real code. This test is the first end-to-end proof that the whole port chain is correct enough to run the actual operating system.

---

## 2026-04-09 — C64 chip port wave + archive cleanup

**Type:** ingest + lint
**Source:** Four-chip C64 port wave (`mos-6502`, `mos-cia-6526`, `mos-sid-6581`, `mos-vic-ii`) followed by the archive cleanup commit once every chip had a verified replacement with a passing test suite.
**Pages created:** `chips/mos-sid-6581.md`, `chips/mos-vic-ii.md`
**Pages updated:** `chips/mos-cia-6526.md` (flipped from "planned" to "ported"), `decisions/archives-as-source.md` (per-subsystem table now marks every C64 chip as ported with commit hashes; added the second cleanup-history row; added a "how to read deleted paths" note pointing at `git show`), `index.md` (three chip pages added to the Chips section)
**Key decisions:** Each C64 chip went through the same "port → verify → stub wiki → commit" loop. The cleanup is a deliberate second pass that happens after *all* the replacements are landing and verified, not interleaved with the ports themselves — this keeps the audit trail coherent (one cleanup commit in each archive, one doc commit in the Emu198x wiki, all referencing each other). Emu198x-backup was consulted as a cross-reference during every chip port but not deleted — it remains the second-opinion reference for future chip work.
**Emu198x commits:** `2d42f8b` (mos-6502 tick), `cf7d0e7` (mos-cia), `49128bf` (mos-sid), `7ac5a65` (mos-vic-ii), plus the cleanup doc commit that bookends this entry.
**Archive commits:** `Emu198x-archive` `6bdc617d3a` (removed 5 crates); `Emu198x-archive-april2026` `bd942d9` (removed cpu-6502).

---

## 2026-04-09 — Archive source correction + C64 chip source map

**Type:** lint + ingest
**Trigger:** Post-`mos-6502` session planning — grepped the wiki for "which archive should the CIA come from" and nothing surfaced, because per-chip sourcing was never written down. Then the `archives-as-source.md` decision record said `Emu198x-backup` was *"probably nothing useful"*, which turned out to be wrong — the backup has functional `cia.rs` / `sid.rs` / `vic_ii.rs` / `c64.rs` implementations in `systems/c64/src/`.
**Pages updated:** `decisions/archives-as-source.md` (added per-subsystem source map, corrected the Emu198x-backup table row and "Best for" column, left an audit-trail note recording the correction), `index.md`
**Pages created:** `chips/mos-cia-6526.md` (stub with pin-contract sketch, port sources, subsystems, test plan — to be fleshed out during the port session)
**Key decisions:** For each C64 chip, the primary source is whichever archive has the most complete implementation (March archive for CIA / SID / VIC-II; April archive for CPU, already ported); the backup is a second reference for chip-level code that wasn't acknowledged before. Future sessions should consult the per-subsystem table before porting any chip.

---

## 2026-04-08 — Phase 0.6 / 0.7 architectural decisions

**Type:** ingest
**Source:** Phase 0 refactor wave — `SpectrumDriver` trait (0.6, commits `fc657b5` + `a3c1e48`) and `Peripheral` trait (0.7, commit `8cfdee1`).
**Pages created:** decisions/spectrum-driver.md, decisions/peripheral-trait.md
**Pages updated:** index.md
**Key decisions:** Within the Spectrum family, one shared run loop via a provided-method trait with `#[inline(always)]` hooks — a measured requirement, not a stylistic preference. Peripheral integration uses static dispatch (typed fields per machine), not a `Vec<Box<dyn Peripheral>>`, because the hot path is inliner-sensitive and every peripheral is known at compile time. Memory-bus intercepts (Beta disk TR-DOS ROM, Interface 1 shadow ROM, Multiface banking) deliberately stay machine-side until a second consumer justifies adding `read_memory` to the trait.

---

## 2026-04-05 — Wiki audit and sync

**Type:** lint
**Findings:** Wiki was behind by ~6 commits. Missing: nec-upd765a chip crate (22nd crate), .SNA snapshot format, WD1793 now functional (was stub), .TRD/.DSK disk support, ZIP archive loading, Timex SCLD hi-res modes (704px framebuffer). Serde partially applied (chips yes, Z80/machines not yet).
**Pages created:** chips/nec-upd765a.md
**Pages updated:** systems/spectrum/overview.md, tests/spectrum.md, decisions/save-state-format.md, index.md

---

## 2026-04-05 — Infrastructure decisions (GUI, serialisation, run loops)

**Type:** ingest
**Source:** Brainstorm Q&A — open questions for Phase 1
**Pages created:** decisions/native-ui-strategy.md, decisions/save-state-format.md, decisions/system-specific-run-loops.md
**Key decisions:** Platform-native frontends long-term, SDL2+native menus for October. serde/bincode for save states. No universal run loop — each system matches its hardware. run_frame() is the system trait boundary.

---

## 2026-04-05 — Long-term system coverage brainstorm

**Type:** ingest
**Source:** Brainstorm continuation — beyond October
**Pages updated:** decisions/product-roadmap.md, index.md
**Key decisions:** Rebuild all 35+ systems at new accuracy bar. Per-system standalones + unified launcher. Wave 2 by historical significance (Atari 2600, BBC Micro, MSX, Master System). All CPU cores cycle-perfect. Chip reuse map documented.

---

## 2026-04-05 — Product roadmap brainstorm

**Type:** ingest
**Source:** Brainstorm session — bridging accuracy to product
**Pages created:** decisions/product-roadmap.md
**Pages updated:** index.md
**Key decisions:** Four Code198x platforms (Spectrum→C64→NES→Amiga), same accuracy bar as Spectrum for all, capture pipeline + CRT as must-haves for October, WASM post-launch.

---

## 2026-04-05 — Initial seed

**Type:** ingest
**Sources:** Emu198x memory files, ARCHITECTURE.md, RULES.md, SPECTRUM-VARIANTS.md, brainstorm docs
**Pages created:** 20 pages across chips/, systems/spectrum/, concepts/, decisions/, tests/, references/
**Notes:** Migrated accumulated knowledge from flat memory files into cross-referenced wiki structure. All content verified against current codebase state (12 crates, 11,500 lines, all 6 Spectrum variants booting).
