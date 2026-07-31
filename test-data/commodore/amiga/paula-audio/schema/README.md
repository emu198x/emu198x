# Evidence Schemas

The suite schema describes deterministic build output. The capture schema
describes one producer's recording and semantic measurements for one case.

A capture record preserves:

- the exact ADF and payload;
- producer family and revision;
- machine, firmware, and execution configuration;
- capture domain, filtering, resampling, and file identity; and
- measured frequency, stereo levels, dominance, and any paired amplitude
  ratio.

The schema does not turn one observation into an expected result. Promotion
requires the evidence review described by the project verification process.

## Related files

- [`suite-v1.schema.json`](suite-v1.schema.json) defines generated manifests.
- [`capture-v1.schema.json`](capture-v1.schema.json) defines observations.
- [`../README.md`](../README.md) defines the capture contract.
