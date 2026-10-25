# The Windows menu attachment is a second, one-call unsafe exception

**Status:** Adopted 2026-10-25 (Steve). Applies to `crates/emu198x-win-menu`
only. Every other crate except `emu198x-apple` keeps the workspace's
`unsafe_code = "forbid"`.

## The problem

The shared native menu was built on every desktop platform but attached
only on macOS. On Windows, `AppMenu::install` was an empty function with a
`TODO(#549)`, so Windows users had the keyboard shortcuts and no menu bar.
[`host-ui-model.md`](host-ui-model.md) needs every host's menus built from
the command catalogue, so the gap has to close.

`muda` attaches a menu to a Win32 window with `Menu::init_for_hwnd`. It is
an `unsafe fn`, because it trusts that the handle names a live window.
There is no safe form. The workspace forbids unsafe code, and
[`apple-bridge-unsafe-exception.md`](apple-bridge-unsafe-exception.md) says
that unsafe code in any other crate needs its own decision. This is that
decision.

## The decision

The call lives in its own crate, `emu198x-win-menu`, so `emu198x-ui` keeps
`forbid`. The crate:

- is empty on every platform but Windows, and depends on `muda` and
  `winit` only there;
- copies the workspace lint table with `unsafe_code` lowered to `deny`,
  and adds `unsafe_op_in_unsafe_fn` and
  `clippy::undocumented_unsafe_blocks` at `deny`;
- allows unsafe code on its `attach` module alone. That module holds one
  unsafe block, the `init_for_hwnd` call, with its safety argument above
  it.

`tests/lint_exception.rs` enforces this. It fails if any workspace lint is
missing or weakened, if `allow(unsafe_code)` appears anywhere but on the
`attach` module, if another source file uses `unsafe`, or if `attach.rs`
gains a second unsafe block.

`emu198x-ui` calls `emu198x_win_menu::attach(menu, window)` from
`AppMenu::install` on Windows. If it fails, the app reports a warning and
runs without a menu; the shortcuts still reach every command.

## Why a crate and not a module

An exempt module would have lowered `emu198x-ui` from `forbid` to `deny`.
That is the crate that grows with every host feature, so it is where a
hard guarantee is worth the most. A separate crate keeps the exception
at the size of the one call it exists for, as the Apple bridge does.

## Drift triggers

Stop and re-read this record if you are about to:

- **Add a second unsafe block to `emu198x-win-menu`,** or any code that is
  not the menu attachment. Another Win32 call needs its own decision.
- **Add `unsafe` to `emu198x-ui`** to avoid the crate boundary.
- **Edit the crate's lint table** without matching the workspace's.

## Related

- [`apple-bridge-unsafe-exception.md`](apple-bridge-unsafe-exception.md):
  the first exception, which this one mirrors
- [`host-ui-model.md`](host-ui-model.md)
- [`native-ui-strategy.md`](native-ui-strategy.md): `muda` is a permitted
  thin wrapper over the platform's own menu

## Log

| Date | Event |
|------|-------|
| 2026-10-25 | Adopted. Steve chose a separate crate over an exempt module in `emu198x-ui`. |
