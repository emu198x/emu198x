# Original Commodore PET 2001, assembly 320008

One physical 6540-ROM / 6550-SRAM board owner. The current profile fits 8 KiB
of RAM and uses the original discrete 40-column video. Other static-board
assemblies (320081, 320132, 320137) are separate machines; the later dynamic-RAM
2001N is owned by `machine-commodore-pet-2001n-32k`.

The family runtime profile is `commodore-pet-2001-320008-8k`; its machine ID is
`commodore-pet-2001-320008`. Source: Commodore schematic 320026 sheets 2/3,
common logic 320130 sheet 1, and original parts cross-reference in the shared
`reference/by-system/commodore-pet/` library. VICE supplies implementation
cross-checks, not hardware authority.

RAM occupies 0000–1FFF. The same 1 KiB display store repeats across
8000–8FFF. The original E800–EFFF I/O decoding selects each PIA or VIA by its
physical select bit, including simultaneous selections; no CRTC is fitted.
PIA 1 CA2 controls original video blanking. Released CPU data-bus values stay
visible in unfitted windows. The runtime uses BASIC 1's own pointer and keyboard
buffer addresses for PRG injection.

The identified original ROM set boots with both the initial C000 image and
its revision, shows 7167 bytes free, accepts typed BASIC and runs a loaded PRG.
Tests check all 63 printable keycaps/back-arrow and SPACE against original
BASIC 1, full native startup/result frames and exact save continuation. No ROM
is bundled. The shared family writes v6 saves and rejects v5 before decoding.
External IEEE devices and the shared PIA chip's pulse/handshake output phases
remain unqualified. Existing set/reset control behaviour is covered.
