# Obtain independent programmed Seek timing

Lane: Spectrum launch-hardening. Follow Emu#1970; no runtime/schema changes pending approval. Reuse the existing authored standalone Fuse host and installed MAME direct-port Lua path. Never edit vendored controller/drive sources.

1. Run four SRT encodings0/1/E/F at4MHz. Observe journeys0→1,1→3,3→1,1→1, twice. Fuse can expose actual drive/PCN transitions and its completion interrupt. MAME exposes public SIS completion through bounded polling; retain its polling resolution and do not invent internal step events.
2. Retain exact commands/tool/compiler/executable/source/firmware hashes, raw observations and process bounds. Source-build and installed-binary identities remain distinct; this is component/direct-port reference execution, not ROM loading or hardware measurement.
3. Check expected programmed adjacent spacing independently of first/final offsets. Require exact nonzero cases, repeat agreement within observation resolution, status/PCN recovery, and a deliberately wrong interval rejection. Record emulator disagreements honestly.
4. Publish a small evidence PR, leave operation tickets open, clean owned workspaces/builds/private ROM directories after retaining verified artifacts.
