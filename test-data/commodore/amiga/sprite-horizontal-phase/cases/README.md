# Case Definition

`cases.json` is the canonical source record for the single fixed-sprite
question. It records the exact register words, DMA-backed image data, marker
geometry, capture line, ready rule, and applicable PAL chipset profiles.

Programmed coordinates are inputs, not expected output coordinates. In
particular, the case does not translate `SPR0POS` and `SPR0CTL` into a golden
capture sample. `expected.status` remains `unresolved`, and its observation
array remains empty, until independently reviewed evidence is retained outside
the source definition.

The builder validates the fixed register relationships, vertical data length,
marker bounds, identities, profile list, and capture bounds before producing
an artifact.

## Related files

- [`../src/README.md`](../src/README.md) explains how the record becomes a
  running probe.
- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines
  the generated manifest.
- [`../references/README.md`](../references/README.md) defines where observed
  results belong.
