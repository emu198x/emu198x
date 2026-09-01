# Case Definition

`cases.json` is the canonical source record for the single adjacent-MOVE
question. It records the fixed OCS register words, DMA-backed marker geometry,
four-colour Copper sequence, measurement line, ready rule, and PAL profile.

Programmed coordinates are inputs, not expected output coordinates. In
particular, the case does not translate the Copper WAIT or marker bit into a
golden capture sample. `expected.status` remains `unresolved`, and its
observation array remains empty, until independently reviewed evidence is
retained under `references/`.

The builder validates the display geometry, short-fetch boundary, colour
identities, back-to-back schedule, profile, and capture bounds before
producing the artifact.

## Related files

- [`../src/README.md`](../src/README.md) explains how the record becomes a
  running probe.
- [`../schema/suite-v1.schema.json`](../schema/suite-v1.schema.json) defines
  the generated manifest.
- [`../references/README.md`](../references/README.md) defines where observed
  results belong.
