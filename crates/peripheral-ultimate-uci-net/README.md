# peripheral-ultimate-uci-net

The **network target** of the Ultimate Command Interface, as 1541 Ultimate-II(+),
Ultimate-II+L, Ultimate 64 and Commodore 64 Ultimate firmware expose it.

The UCI carries several targets — DOS, file and control among them. Only the
network one is modelled here, which is what the `-net` in the name is for: a
crate called `ultimate-uci` would promise the rest of the interface too.

It is a peripheral rather than a chip: something plugged into a machine, in the
same sense as [`peripheral-kempston-joystick`](../peripheral-kempston-joystick),
not a part soldered inside one.

Software talks to it through four registers rather than by timing a serial
line: write a command, push it, read the response, accept it. The device owns
the socket, so the machine has no bit periods to hold, no framing to
resynchronise and no interrupt to service — which is why a client that can use
this should, and why the bit-banged user port is the fallback rather than the
other way round.

It knows nothing about the machine it is plugged into. A host supplies register
reads and writes at whatever addresses that machine decodes — `$DF1C`-`$DF1F`
on a C64 — and calls `poll` to let the transport breathe.

```rust
use peripheral_ultimate_uci_net::UltimateUciNet;

let mut uci = UltimateUciNet::new();
let identification = uci.read(UltimateUciNet::REG_COMMAND); // 0xC9
```

## Register map

| Offset | Write | Read |
|--------|-------|------|
| 0 | control: push / accept / abort / clear-error | status: data available, status available, state |
| 1 | command byte | identification (`0xC9`) |
| 2 | — | response byte |
| 3 | — | status byte (ASCII, `"00"` leads a success) |

## Commands

Target `0x03` is the network. `OPEN_TCP` returns a socket handle, `READ`
returns a little-endian length followed by that many bytes, `WRITE` returns the
number of bytes accepted, and `CLOSE` releases the socket.
