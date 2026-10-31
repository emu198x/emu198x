# Original SNES v1 states

Captured before extracting the regional owners, with project-written cartridges
only. All three stable profiles have progressive and interlaced specimens.
Guest 65C816 writes enable video and arm a WRAM-to-VRAM DMA; the saved state is
partway through the transfer. Held/partially shifted pad contacts, SRAM, active
DSP noise and scheduler/APU phases are retained.

Each output contains three native frames, exact elapsed master clocks, audio
sample bits and the final full state. The regression compares both direct and
restored continuations. Capture only into a new directory with
`EMU198X_SNES_CAPTURE`; existing captures cannot be overwritten.
