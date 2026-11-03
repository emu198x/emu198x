# Original PET board and live second PIA use v6

Adopted 2026-11-03 for the original PET 2001 portion of #355. Steve explicitly
approved restoring the missing second onboard 6520 PIA before adding the
assembly-320008 8 KiB owner, and writing v6 saves rejecting v5 for the PET
family. Missing live PIA history cannot be reconstructed exactly. Reject
legacy versions before positional decoding or mutation; retain original v5
specimens byte-for-byte. No legacy header is relabelled in production.

Commodore schematic 320026 identifies assembly 320008 with 6540 ROMs and
6550 RAM. Common logic sheet 320130 and the original parts cross-reference
identify both PIAs. New owner/profile IDs must distinguish this board from
the later 320349 PET 2001N, and existing IDs stay stable. Other original static
boards with different parts are separate future owners.

The second PIA owns its register/interrupt state, onboard IEEE data readback
and control-line connections. With no external devices, released lines pull
high. External IEEE devices remain #349. The existing PIA chip's unsupported
pulse/handshake output phases are not qualified by firmware using set/reset
outputs; do not describe this as complete IEEE-488 peripheral support.

Primary documents and acquisition identities belong in umbrella reference
`by-system/commodore-pet/commodore-pet-320008-board-schematics.*` and
`commodore-pet-2001-main-logic-parts.*`; VICE is implementation precedent.

The physical owner is named `machine-commodore-pet-2001-320008`, with machine
ID `commodore-pet-2001-320008` and RAM-fitting profile
`commodore-pet-2001-320008-8k`. Assembly-based naming keeps later RAM profiles
on the same board owner. v6 stores only the fitted RAM length (8 or 32 KiB),
validated against the selected physical configuration. No phantom RAM is
allocated for the original 8 KiB fitting. Existing v5 specimens remain
unchanged; all three previous owners retain their frozen native pixels,
audio and clocks independently of the new positional v6 state layout.
