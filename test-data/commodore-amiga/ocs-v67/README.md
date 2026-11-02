# Original OCS-shaped v67 continuations

Fifty-six cases were captured and replayed exactly from untouched production
`c06321677cde6b4aea0211197eb8436ceae0a1a7` before correcting A2000 RTC decode
or extracting fixed owners. Twelve existing profiles cover PAL/NTSC,
progressive/interlaced output, boundary and real debugger half-CCK cuts, A501
and A530 configurations. A1000 specimens cover both bootstrap-visible and
guest-executed WOM lock/handoff phases. Guest code drives controls and audio;
no CPU/RAM patch supplies the A1000 handoff. Firmware is project-owned.

`original-v67.zip` losslessly stores nine firmware images plus 56 complete
initial saves, observation streams and final saves. `manifest.json` records
each original byte count and SHA-256, plus the archive identity. All 177
decompressed files were compared byte-for-byte/hashes before removing the
1.27 GB raw duplicate. Runtime tests read and validate members in memory.
Do not regenerate or overwrite originals after production changes.

The exact capture helper and its successful log are retained in the docs repo's
`plans/2026-11-02-amiga-remaining-board-owners/`. To reproduce, check out the
original production commit, copy that original helper into the runtime tests,
and use a new `EMU198X_OCS_CAPTURE` directory. The helper checks HEAD, refuses
tracked production changes and creates every original with create-new-only.
