# Shared Sord M5 wiring

`M5Core<B>` wires the Z80, TMS9918-family VDP, SN76489 and CTC. The two
private machine crates bind `B` to one board at compile time. Common code
cannot choose a region during construction; family selection belongs to
`runtime-sord-m5`.

The live state retains the former `machine-sord-m5` serialized field order.
Concrete board deserializers check its region, raster budget and geometry.
The runtime adds no dispatch tag, preserving version-5 saves. Frozen pre-split
states in `test-data/sord-m5/split-v5` verify full CPU/state, pixel and audio
continuation for both boards. The changelog here retains the shared machine's
history.

This split preserves the existing hardware model, including the legacy PAL
PSG clock stored in older version-5 states (#252). It does not establish new
hardware timing measurements or add the currently unimplemented expansion,
cassette and printer hardware.
