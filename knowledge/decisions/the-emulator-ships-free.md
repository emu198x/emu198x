# The emulator ships free

**Status:** Adopted 2026-10-20 (Steve). Applies to every host and channel:
the browser player, the desktop binaries and their installers, and any
future smart-TV, macOS or Apple TV build, including store releases.

## The decision

Emu198x itself is free to obtain and use on every platform. No host or
build charges for the emulator.

Steve, 2026-10-20, during the Apple TV discussion: "In all cases, we'll
ship the emulator itself as free."

The only paid offering under discussion is licensed content: game packs
that rights holders could offer as one-off in-app purchases inside a free
per-system app (emu198x#1898). This record does not authorise those packs,
any outreach to rights holders, or a store release. Each of those still
needs its own agreement.

## What this does not settle

**GPL-2.0-or-later and App Store terms.** Giving the app away does not
remove the conflict between the GPL and the store's usage terms. VLC was
free when it was removed from the App Store over that conflict in 2011.
Steve's sole authorship of the commits does not settle it either. Under
[`crate-licensing-split.md`](crate-licensing-split.md), the workspace
contains code ported from third-party GPL sources, such as the
reSID-derived tables in `emu198x-mos-sid-6581`. A store exception from
Steve covers only code whose copyright Steve holds, not code whose
copyright belongs to other authors. Any store release needs a provenance audit of
the crates it ships, and either a source-by-source exception or a
store-compatible route.

**The Spectrum ROM alongside paid content.** A free app that bundles the
48K ROM uses Amstrad's permission the same way the browser player does
(see [`test-rom-policy.md`](test-rom-policy.md)). Paid packs that run on
that ROM are a further step: the permission is informal and dates from
1999, and since the 2007 BSkyB acquisition nobody has established who
holds those rights now. A deal with a rights holder should cover firmware
rights alongside game rights.

## Drift triggers

Stop and re-read this record if you are about to:

- **Propose a paid edition, paid tier or price for any Emu198x build.**
  The emulator is free everywhere. Only licensed content can be paid.
- **Treat "it's free" as resolving a licensing question.** Price does not
  affect the GPL/App Store conflict or a firmware permission's scope.
- **Read this record as approval for game packs or a store release.** It
  decides the emulator's price, nothing more.

## Log

| Date | Event |
|------|-------|
| 2026-10-20 | Adopted. Stated by Steve while reviewing the TV and Apple host ideas (emu198x#1895–#1898). |
