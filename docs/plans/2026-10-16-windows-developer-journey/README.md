# Windows developer journey: blocked at emulator startup

Released Asm198x and Build198x produce the maintained Meteor Storm tape on
native Windows. Released Spectrum Emu198x 0.34.2 and 0.35.0 both overflow the
main-thread stack on a script containing only `query_cpu`. The complete
package-to-boot journey is **not qualified**.

[Emu issue #1792](https://github.com/emu198x/emu198x/issues/1792) tracks the
startup failure. [PR #1791](https://github.com/emu198x/emu198x/pull/1791) contains
the opt-in workflow, pinned inputs, harness and rejection controls.

## Native observations

| Run | Emu | Result |
| --- | --- | --- |
| [38920027025](https://github.com/emu198x/emu198x/actions/runs/38920027025) | 0.34.2 | Harness rejected Windows checkout CRLF before invoking tools. |
| [38920539428](https://github.com/emu198x/emu198x/actions/runs/38920539428) | 0.34.2 | Exact Git-blob staging passed; CP1252 could not retain a help-output arrow. |
| [38921022873](https://github.com/emu198x/emu198x/actions/runs/38921022873) | 0.34.2 | UTF-8 retention, tool invocations and tape validation passed; emulator overflowed its stack. |
| [38921616978](https://github.com/emu198x/emu198x/actions/runs/38921616978) | 0.34.2 | Reduced startup-only query reproduced the overflow without media or frames. |
| [38923295182](https://github.com/emu198x/emu198x/actions/runs/38923295182) | 0.35.0 | Current stable release reproduced the same startup-only overflow. |

The first two failures exposed harness defects, corrected on this branch.
None of these failed runs has been reclassified as passing. Six unit controls
pass locally and in the final Windows run. They cover rejection by the harness;
they do not substitute for execution of the game or the missing-CODE tape.

All runs use native Windows Server 2022 AMD64. The final report records
Python 3.14.8, Asm 0.0.67, Build 0.2.12 and Emu 0.35.0, plus archive, executable,
firmware and source identities. The sample revision stays fixed at
`51297ac3196a990286b0d21a4dedd4653c0168f0`.

## What passed and what remains unknown

The final three runs execute every packaging command successfully. The tape
contains six checksum-valid blocks: BASIC, SCREEN$ and CODE header/data pairs.
Names, lengths, autorun line and load addresses match the maintained recipe.
The SCREEN$ payload matches the authored screen; CODE matches a separate raw
assembly. Their tape SHA-256 is
`31420966522468be267830cf1b0ce5fe31087b987dfe293d879512a67de4933d`.

The startup-only probe exits `3221225725` (`0xC00000FD`). Its stderr states
`thread 'main' … has overflowed its stack`. Both published Emu executables
reserve 1,048,576 bytes of stack. The allocation exhausting the stack has not
been isolated, and no published binary was patched.

Game boot, loaded title state, rendered title pixels, the screenshot and the
real missing-CODE runtime control remain untested. GUI startup, installers and
learner effectiveness are outside this check. The unchanged full journey must
run after the startup failure is fixed before Windows can be qualified.

## Retained evidence

Each `run-*` folder holds the downloaded artifact bytes and a `run.json`
snapshot of the run, jobs, steps and artifact metadata. The first run also has
a retained text log. `sha256.json` identifies the retained evidence bytes.
The earlier CP1252 help files remain in their original encoding.

The firmware identity check, firmware cleanup and evidence upload pass in
every run. The artifacts contain reports, command output, scripts, symbols and
generated sample media. They contain no firmware bytes, released executables
or downloaded archives. Git retention preserves these observations beyond the
hosted artifacts' 30-day retention period.
