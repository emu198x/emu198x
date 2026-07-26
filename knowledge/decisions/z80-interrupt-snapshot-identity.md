# How does a restored Z80 resume an accepted interrupt response?

**Status:** binding (2026-07-25).

## Context

The Z80 executes each instruction and interrupt response through a static
`MStep` sequence. The walker stores a reference to that sequence, but a
`&'static` slice is deliberately skipped by serde and must be reconstructed
after decode.

An opcode prefix and opcode identify ordinary instruction sequences. They do
not identify an accepted NMI or distinguish the IM 0, IM 1, and IM 2 response
sequences. A snapshot taken during one of those responses therefore retained
the phase and staged data but could resume against the wrong static sequence.

## Decision

The existing serialised walker prefix enum also carries sequence identity.
Four variants are appended after its original variants:

- accepted NMI;
- accepted IM 0;
- accepted IM 1;
- accepted IM 2.

Appending preserves the postcard discriminants of existing prefix values. It
also avoids adding another field to every serialised Z80.

Interrupt acceptance records the corresponding identity before entering the
response. Beginning the next instruction clears it to the ordinary no-prefix
value. Restore checks the interrupt identities first when reconstructing the
skipped walker sequence; execution dispatch uses the same typed identity.

All durable Z80 runtime envelopes advance from version 2 to version 3. Version
2 is rejected by probing the leading postcard version before decoding the
remaining payload because it cannot reliably reconstruct a response already
in progress.

## Consequences

Snapshots may be taken at any externally observable half-cycle of NMI, IM 0,
IM 1, or IM 2 and continue from the preserved response state.

The prefix enum now represents static sequence identity as well as an opcode
prefix. New serialised variants must continue to be appended so existing
postcard discriminants remain stable.

Every machine that deserialises a Z80 must call
`Z80::rehydrate_walker_sequence` before ticking it.

## Verification

The Z80 tests serialise and restore every half-cycle of each interrupt response,
then compare the continuing CPU and memory states. The Spectrum runtime has a
separate regression that snapshots during NMI acknowledgement, restores
through the public runtime envelope, and advances both machines in lockstep.

## Related documents

- [Save-state: serde the live machine](savestate-live-machine-serde.md)
- [Half-cycle signals](half-cycle-signals.md)
- [CPU bus interface](cpu-bus-interface.md)
- [Spectrum architecture review](spectrum-architecture-review.md)
