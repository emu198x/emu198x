# emu198x-esp-at-modem

A cycle-accurate ESP-AT WiFi modem hanging off a bit-banged 8N1 serial line,
for 8-bit emulators.

Vintage machines with no UART reach the network by bit-banging two port pins
and talking to an ESP8266/ESP32 running Espressif's AT firmware. This crate
models that link: the 8N1 bit timing, the subset of ESP-AT those clients use,
and an optional real TCP transport behind it.

It speaks the two AT dialects those machines actually meet — Espressif's ESP-AT
(`AT+CIPSTART`, `AT+CIPSEND`, `+IPD,`) and Hayes as a Zimodem-based user-port
modem speaks it (`ATD`, then a transparent link) — latching whichever the
computer uses first. A client only ever speaks one, so accepting both cannot
confuse either.

It knows nothing about the machine it is attached to. The emulator supplies the
transmit level each tick and reads the receive level back, so the wiring stays
with the machine — VIA CB2/PB0 on a VIC-20, CIA2 PA2/PB0 on a C64 — and only
the timing, protocol and transport live here.

```rust
use emu198x_esp_at_modem::EspAtTcpBridge;

// 8 cycles per bit, 64-byte frames.
let mut bridge = EspAtTcpBridge::new(8, 64);

// Each emulated CPU cycle: hand it the transmit level, take the receive level.
let rx = bridge.tick(tx_level);
```

## Diagnostics

Enable the `query` feature to expose `EspAtTcpBridge::QUERY_LEAVES` and
`query_leaf`, the names this peripheral answers about itself — whether it is
connected, its last transport error, and the bytes it has received. A host
mounts them under a path of its choosing and drops them again when the modem is
unplugged, so the names live with the hardware rather than with each machine.
The feature is off by default and is the crate's only dependency.

Timing is expressed in emulated CPU cycles rather than wall-clock time, so
recordings and tests reproduce exactly regardless of host scheduling — which
matters, because a host-paced link against a free-running emulator produces
timing that varies with load.

## Layers

- `BitBangSerial` — physical 8N1 levels in and out, driven by cycle count.
- `EspAtModem` — the deterministic ESP-AT command subset (`AT+CIPSTART`,
  `AT+CIPSEND`, `+IPD` framing), with no transport.
- `EspAtTcpBridge` — the above, backed by a real `TcpStream`.

Extracted from the Emu198x VIC-20 runtime once a second machine needed it.

## Licence

GPL-2.0-or-later.
