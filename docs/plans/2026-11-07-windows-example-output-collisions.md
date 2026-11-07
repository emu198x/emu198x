# Give every workspace example a distinct output filename

Lane: engineering-frontier build and validation reliability. No emulator
behaviour, dependency or test-coverage changes.

## Reproduced failure

Windows main CI run 42391205445, job 128459399375, warns that examples share
`target/debug/examples/parity.exe` and `parity.pdb`, then linking the NES
example fails with `LNK1104: cannot open file ... examples\\parity.exe`.
Cargo metadata independently finds two colliding names among 31 examples:

- `parity`: emu198x-commodore-web, emu198x-game-boy-web, emu198x-nes-web.
- `board_baseline`: runtime-nintendo-game-boy, runtime-acorn-bbc-micro,
  runtime-commodore-vic-20, runtime-msx, runtime-sega-master-system,
  runtime-sord-m5, runtime-spectravideo-svi-328.

These targets share unqualified executable and Windows PDB output paths.
The same collision warnings appear for both groups in the failed log.

## Steps

1. Keep all example source paths. Give the ten colliding examples explicit,
   machine-qualified `[[example]]` names in their package manifests. Check
   Cargo's discovery result and preserve all 31 source paths, including the
   existing NES `dash_parity` example; disable automatic discovery only if
   explicit target paths are otherwise discovered twice.
2. Update the exact native callers: `web-player/check-worker.mjs`, the two
   Commodore README commands and the NES README command. Source-path citations
   in retained fixture documentation remain valid.
3. Add a small Cargo-metadata duplicate-output check to CI after toolchain
   setup. Its self-test must reject duplicate example names and empty input;
   the unchanged workspace must fail before the fix and pass afterward.
4. Compare before/after metadata: exactly the ten intended names change,
   every example source path and target property otherwise remains. Build
   all affected example packages together, run existing parity examples with
   repository fixtures where available, formatting and diff checks. Require
   the existing postmerge Windows workspace job to verify the original
   platform failure is resolved. Preserve the Linux-only pull-request policy;
   do not claim Windows confirmation before that job passes.
5. Push a reviewed PR for coordinator approval before merge. Retain receipts
   and remove the task worktree/build cache after verified remote retention.

## Verified target names

| Package | Old name | New name |
|---|---|---|
| runtime-nintendo-game-boy | board_baseline | game_boy_board_baseline |
| emu198x-commodore-web | parity | commodore_web_parity |
| emu198x-nes-web | parity | nes_web_parity |
| emu198x-game-boy-web | parity | game_boy_web_parity |
| runtime-acorn-bbc-micro | board_baseline | bbc_micro_board_baseline |
| runtime-commodore-vic-20 | board_baseline | vic_20_board_baseline |
| runtime-msx | board_baseline | msx_board_baseline |
| runtime-sega-master-system | board_baseline | master_system_board_baseline |
| runtime-sord-m5 | board_baseline | sord_m5_board_baseline |
| runtime-spectravideo-svi-328 | board_baseline | svi_328_board_baseline |

Explicit paths prevent duplicate automatic discovery without disabling it.
All 31 examples and their source paths and contents remain, including NES
`dash_parity`. The metadata guard rejects the original two collision groups
and passes the revised workspace. It runs in the existing Test plan job
after pinned toolchain setup; PR platform policy is unchanged.

## Local verification

All eleven examples in the ten affected packages built together without
collision warnings. Game Boy and NES parity produced three input checkpoints
each; Commodore parity produced all five checkpoints through 360 frames on
both C64 and Amiga. The existing 19 package-selection and seven doctest-runner
tests passed. Formatting, Ruff, Node syntax, caller-target resolution and diff
checks passed. Actual Windows confirmation remains gated on the postmerge
workspace job.
