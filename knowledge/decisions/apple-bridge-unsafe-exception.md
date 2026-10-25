# The Apple bridge may use unsafe code

**Status:** Adopted 2026-10-20 (Steve). Applies to `crates/emu198x-apple`
only. The one other exception is `crates/emu198x-win-menu`, under
[`windows-menu-unsafe-exception.md`](windows-menu-unsafe-exception.md).
Every other crate keeps the workspace's `unsafe_code = "forbid"`.

## The decision

`emu198x-apple` exports a C interface for native Apple hosts (#1896). It
cannot be written without `unsafe`: exporting a function needs
`#[unsafe(no_mangle)]`, and reading the caller's strings and buffers means
dereferencing raw pointers. The workspace's `forbid` cannot be relaxed by
an `allow`, so this crate opts out of the workspace lint table and keeps a
copy of it with one change:

- `unsafe_code` is `deny` instead of `forbid`, and `src/lib.rs` allows it
  on the `ffi` module alone.
- `clippy::undocumented_unsafe_blocks` and `unsafe_op_in_unsafe_fn` are
  `deny`, so every unsafe block states why it is sound.
- Every other workspace lint applies at its workspace level.

`tests/lint_exception.rs` enforces all three. It fails if any workspace
lint is missing or weakened, if `allow(unsafe_code)` appears anywhere but
on the `ffi` module, or if another source file uses `unsafe`.

The `ffi` module only converts arguments and forwards calls to `Fleet` in
`emu198x-fleet-web`. No emulation code moves into it. Each exported
function catches panics so none unwinds into Swift, and a panic poisons the
machine it hit.

## Why

`application-icon.md` left a crate-level lint exception for a separate
decision. This is that decision. Every route from Swift to Rust needs it:
generated bindings such as UniFFI emit the same exported functions and
pointer handling. Running the emulator in a separate process avoids
`unsafe` on the Mac, but tvOS apps cannot start other processes, so that
route closes the Apple TV work (#1898).

## Drift triggers

Stop and re-read this record if you are about to:

- **Add `unsafe` to any other crate.** That needs its own decision.
- **Move emulation logic into `ffi.rs`.** It converts arguments and
  forwards calls, nothing more.
- **Edit the crate's lint table** without matching the workspace's, or add
  a second `allow(unsafe_code)`.

## Log

| Date | Event |
|------|-------|
| 2026-10-20 | Adopted for the Apple native host spike (#1896–#1898). |
| 2026-10-25 | A second exception, for the Windows menu attachment, adopted in its own record. |
