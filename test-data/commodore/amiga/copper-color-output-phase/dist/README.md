# Generated Output

Running `python3 tools/build.py` writes `adjacent-color00-moves.adf`, its raw
payload, and `suite-v1.json` here.

Generated artifacts are ignored because they are reproducible from the source
case, assembly, builder, and recorded toolchain. This README and `.gitignore`
are the only tracked files in the directory.

Capture packages must bind the exact ADF and payload hashes from their local
generated manifest. They must not rely on a filename alone.

## Related files

- [`../tools/README.md`](../tools/README.md) defines the build process.
- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines
  the generated manifest.
- [`../references/README.md`](../references/README.md) defines where capture
  packages belong.
