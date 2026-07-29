# Case Definitions

`cases.json` is the canonical list of questions and register inputs.

Each case changes one control condition or window shape. Register objects carry
both an exact 16-bit word and symbolic flags so a reviewer can check that the
PAL baseline remains constant. Raw horizontal values are deliberately not
translated into output pixels in this file; that relationship is one of the
things captures may establish.

`expected.status` remains `unresolved` until evidence is reviewed outside the
source case definition. The empty `observations` array prevents the build
inputs from becoming an accidental golden-output store.

The builder validates identifiers, numeric ranges, case uniqueness, expected
status, capture bounds, and the relationship between the numeric register
words and their declared flags.

## Related files

- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines the
  generated suite manifest.
- [`../src/README.md`](../src/README.md) explains how each definition becomes a
  running probe.
