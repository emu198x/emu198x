# Shared ColecoVision wiring

`CvCore<B>` binds shared pin-driven wiring to one board at compile time.
The private NTSC/PAL machine crates select fixed video and raster constants;
family selection and dispatch belong to `runtime-coleco-colecovision`.

`CvState` retains the original serialized field order. The runtime forwards
state without an enum tag, preserving version-5 bytes. Concrete deserializers
validate region, raster budget and height. Frozen pre-split observations check
complete continued state, pixels, audio and native field timestamps. The
changelog retains the old shared machine's history. Clock, NMI, controller and
memory behaviour are preserved; this split adds no peripherals or mapper.
