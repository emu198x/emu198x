# Shared SVI-328 wiring

`SviCore<B>` binds the shared Z80, video, AY, PPI, memory and cassette wiring
to a concrete board at compile time. The private NTSC/PAL machine crates fix
video and raster constants; family selection belongs to the shared runtime.

`SviState` retains every original serialized field in order. Runtime dispatch
adds no enum tag, preserving version-7 bytes. Concrete deserializers validate
region, raster budget and height. Frozen pre-split observations exercise full
continued state, pixels, audio, keyboard/joystick input, both memory overlays
and active cassette waveform progress. The cassette module is moved unchanged;
the changelog retains the old shared machine's history. No SVI-318, disk
expansion, new timing model or cassette write support is introduced here.
