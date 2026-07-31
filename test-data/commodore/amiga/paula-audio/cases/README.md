# Cases

This directory defines the controlled Paula register configurations built by
the corpus.

Each case asks one waveform question. Expected observations remain unresolved
until an admissible independent capture is registered. The half-volume case
declares its full-volume comparison case so amplitude ratios cannot be formed
from unrelated producers or machine configurations.

The case source is canonical. Generated, fully expanded records are written to
`../dist/suite-v1.json`.

## Related files

- [`cases.json`](cases.json) is the versioned case source.
- [`../README.md`](../README.md) defines the capture boundary.
- [`../schema/capture-v1.schema.json`](../schema/capture-v1.schema.json)
  defines registered observations.
