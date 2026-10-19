# Give Escape to the emulated keyboard

Engineering-frontier bug fix. Approved by Steve on 19 October: remove Escape-to-quit fleet-wide, send Escape to machines that have it, retain normal window closing and platform Quit.

1. Remove the Escape exit branch and its unused event-loop argument from `crates/emu198x-ui/src/lib.rs`. Leave CloseRequested and native Quit intact. Remove stale Escape-quit instructions from native controls and the halt overlay.
2. Add host Escape mappings to the ten native adapters whose runtime already accepts Escape: Atom, BBC Micro, Electron, Amstrad CPC, Atari 800XL, Amiga, MTX, MSX, SVI-328 and Einstein. On CPC restore Tab's own existing runtime key, replacing the old Escape workaround. Keep machines without an Escape key unchanged.
3. Extend native mapping checks and verify these names resolve to machine input. Test press and release through shared host-key routing. Run affected UI/native tests, Clippy and formatting, and exercise a real window if the desktop is available.
4. Commit and push a separate reviewable PR. Do not merge automatically. No new dependencies, chip timing, firmware or public schemas change.
