# SG-1000 clocks and fixed board owners

Adopted 2026-11-01 under the approved #2144 continuation and explicit v6 agreement.

The PAL configuration uses independent nominal CPU/PSG (3,580,000 Hz) and VDP
(10,738,635 Hz / 2) oscillators. VDP phase accumulates on CPU half-cycles with
an integer rational ratio; native requests still finish at the chip wrap.
NTSC retains its existing 3:2 dot/T-state ratio and original state bytes.
Source: reference/by-system/sega-sg-1000/2026-pal-clock-observations.md.
The inspected Sega SC-3000 schematic and console report support this nominal
model; no physical waveform or every PAL board revision is qualified. MAME's
sc3000pal is clock-circuit precedent, not a full console execution oracle.

V6 saves carry the corrected PAL phase units/configuration. V5 NTSC and early-II
states retain exact readers; v5 PAL states reject before decoding because the
old timing cannot promise exact continuation. Keep all original v5 fixtures
immutable, compare NTSC/II using a test-only original envelope encoder, and
check corrected PAL v6 continuation separately. Validate selected region,
phase and video geometry before installing restored state.

Fixed SG NTSC, PAL and early TMS9918 SG-II owners use common-sega-sg-1000.
The three profile IDs, family runtime and binary remain stable. Later TMS9118
and custom-VDP boards, SC-3000 keyboard/PPI, and cartridge mappers remain
separate qualification/implementation work. The runtime uses a closed machine
`machine.rs` dispatcher as a narrow override of runtime-internal-shape.md's
additional generic axis; shared lifecycle/query/input/snapshot concerns stay
in the existing modules. Fixed owners expose chip/IO operations, never mutable
replacement of the raw board configuration.
