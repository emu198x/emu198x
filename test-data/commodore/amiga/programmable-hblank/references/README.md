# Documentary References

The probe's register addresses and baseline control bits were checked against
the *Amiga Hardware Reference Manual*, Third Edition, Appendix C, particularly
the enhanced-chip-set register map and the sections “Multi-Sync and Bi-Sync
Monitors” and “New BEAMCON0 Register”.

The later-chip questions were also informed by *The Amiga 3000 System
Specification*, sections 3.2.7 and 3.2.8, which describe Lisa horizontal
blanking registers and the finer horizontal comparator resolution.

These works explain which controls exist. They do not settle every observable
edge, gate interaction, wrap behaviour, equal-value behaviour, or capture
convention addressed by this corpus.

No third-party reference text, emulator source, emulator binary, or firmware
is redistributed here. Registered captures contain only the corpus-authored
guard pattern and blanking produced during controlled project runs.
Implementations are evidence producers, not specification authorities. Their
observations belong in capture records with their implementation family made
explicit.

## Related files

- [`../cases/README.md`](../cases/README.md) contains the resulting questions.
- [`../schema/README.md`](../schema/README.md) defines how observations are
  recorded.
- [`comparator-capabilities.md`](comparator-capabilities.md) records which
  audited producers can currently supply admissible evidence.
- [`copperline-0.13.0-eec5806/`](copperline-0.13.0-eec5806/README.md) contains
  the registered Copperline software capture.
- [`fs-uae-5.0.7-f362278c/`](fs-uae-5.0.7-f362278c/README.md) contains the
  registered current-generation UAE-family software capture.
