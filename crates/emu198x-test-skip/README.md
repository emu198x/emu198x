# emu198x-test-skip

Declare a test skipped for a missing fixture, so it cannot pass silently.

A test that needs a ROM, a corpus or a disk image usually guards on it and
returns early. `libtest` then prints `ok`, and nothing tells that apart from a
test that ran and passed. This crate makes the skip visible, and makes it a
failure wherever the fixture is supposed to exist.

```rust,ignore
let Some(rom) = staged_rom() else {
    emu198x_test_skip::skip!("Dragon 32 ROM not staged (EMU198X_DRAGON32_ROM)");
};
```

`skip!` takes `format!` arguments and returns from the test. Say which fixture
is missing and name the variable that would supply it.

## Environment

- `EMU198X_STRICT_FIXTURES`: when set and non-empty, a skip panics. Set it
  where the fixtures should be present, such as a development machine or a
  nightly accuracy run.
- `EMU198X_SKIP_LOG`: a path to append one `<test>\t<reason>` line per skip
  to. `libtest` captures the output of passing tests; a file survives that.

With neither set, a skip writes one line to stderr, starting with
`EMU198X-SKIP`, and the test passes.

[`Policy`](https://docs.rs/emu198x-test-skip/latest/emu198x_test_skip/struct.Policy.html)
and `record_under` take the same decision from explicit values, so the
behaviour can be tested without changing the process environment.

## Provenance

Part of [Emu198x](https://github.com/emu198x/emu198x), a family of
cycle-accurate retro-computing emulator cores. It is published so the cores
that live in their own repositories can share it. It has no dependencies and
is versioned independently of the Emu198x suite.

Licensed GPL-2.0-or-later.
