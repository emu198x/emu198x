# Deferred Work

> Archived document. Do not treat status claims here as current. Current state lives in `../../status/` and binding rules/decisions.


Cross-cutting items not specific to any single system. For per-system status and remaining work, see `docs/systems/`.

---

## Cross-cutting — infrastructure

### CRT shader
- **What:** GPU fragment shader implementing `CrtParameters`. Types and presets defined in `emu-display`.
- **Blocks:** Authentic visual output for all systems.
- **Effort:** Medium-large. WGSL shader, multi-pass rendering.

### Debug panels (egui)
- **What:** Multi-window panel UI with disassembly, registers, memory editor, system-specific views.
- **Blocks:** Interactive debugging for all systems.
- **Effort:** Large. egui integration, per-system view adapters.

### ROM directory scanning
- **What:** Auto-discover ROMs by hash from a configured directory.
- **Effort:** Small.

### Session restore
- **What:** Restore last session on launch (last ROM, window position, settings).
- **Depends on:** TOML persistence (done).
- **Effort:** Small-medium.

### LZ4 rewind compression
- **What:** Compress rewind snapshots with LZ4. Only matters for large-state systems (Amiga).
- **Effort:** Small.

### Multiple save state slots
- **What:** Numbered slots with thumbnails instead of a single save.
- **Effort:** Small.

---

## Cross-cutting — formats

### Niche tape formats (PZX, CSW)
- **What:** PZX and CSW importers for Spectrum.
- **Effort:** Small-medium each.

### TapeInputPath as distinct type
- **What:** Revisit now that three systems use emu-tape (Spectrum, Dragon, C64 datasette).
- **Effort:** Small.

### Additional image formats (BMP, JPG, WebP)
- **What:** Screenshot formats beyond PNG.
- **Effort:** Small.

---

## Cross-cutting — low priority

### Reference acquisition
- **What:** Download freely-available reference documents into `refs/`.
- **Effort:** Small (manual).
