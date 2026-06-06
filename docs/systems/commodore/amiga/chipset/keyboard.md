# Keyboard Controller — Serial Protocol, Power-Up, Handshake

The Amiga keyboard is a standalone device with its own 6500/1 microprocessor.
It handles key scanning internally and communicates with the host via a serial
protocol through CIA-A's SP and CNT lines.

## Serial Interface

| Signal | CIA-A Pin | Direction | Purpose |
|--------|-----------|-----------|---------|
| KDAT | SP (serial port) | Keyboard → Host | Serial data |
| KCLK | CNT (counter) | Keyboard → Host | Serial clock |

The keyboard clocks out 8 bits per byte on KDAT, with KCLK providing the shift
clock. CIA-A's serial shift register captures the bits automatically. When 8
bits are received, CIA-A sets ICR bit 3 (SP) and fires an interrupt.

## Keycode Encoding

The keyboard does not send raw keycodes. Each byte is transformed before
transmission:

```
KDAT = NOT(keycode ROL 1)
```

That is: rotate the keycode left by 1 bit, then bitwise invert. The CIA
captures the inverted bits; software decodes by inverting and rotating right:

```
keycode = NOT(SDR) ROR 1
```

This encoding scheme dates from the original A1000 keyboard. It survives in all
subsequent models.

### Key-Down and Key-Up

- **Key-down:** Bit 7 clear. Raw keycode = $00–$7F.
- **Key-up:** Bit 7 set. Raw keycode = $80–$FF.

The encoding is applied to the full byte including the up/down bit.

## Power-Up Sequence

At power-on, the keyboard runs a self-test, then announces itself to the host:

```
T = 0ms:        Power-on reset. Keyboard starts internal self-test.
T ≈ 200ms:      Self-test complete. Keyboard begins power-up sequence.
T ≈ 200ms:      Send $FD (init power-up stream).
                 Wait for host handshake.
T + handshake:  Send $FE (terminate power-up stream).
                 Wait for host handshake.
                 Power-up complete. Keyboard enters idle state.
```

### Timing Constants

| Constant | E-clock ticks | Wall time | Purpose |
|----------|--------------|-----------|---------|
| Power-up delay | 150,000 | ~211ms | Delay before first byte |
| Byte interval | 700 | ~1ms | Minimum gap between bytes |
| Handshake timeout | 100,000 | ~141ms | Resend if no handshake |

### Timeout Resend

If the host does not handshake within ~141ms (100,000 E-clock ticks), the
keyboard resends the current byte. During boot, KS typically does not enable
the CIA-A SP interrupt until well after the keyboard starts sending. This means
the keyboard may send 20+ bytes (resending $FD and $FE) before the host
acknowledges. This is normal and expected.

## Handshake Protocol

After receiving a byte, the host acknowledges by toggling CIA-A's serial port
direction:

1. Set CIA-A CRA bit 6 = 1 (switch SP to output mode)
2. Wait at least 75µs (~53 E-clock ticks)
3. Set CIA-A CRA bit 6 = 0 (switch SP to input mode)

The falling edge on the SP line signals the keyboard to send the next byte.

### Handshake Detection

In the emulator, handshake detection uses edge detection on CIA-A CRA bit 6.
The Amiga struct tracks the previous value of CRA bit 6. A transition from
1 → 0 (output mode → input mode) triggers the handshake:

```
if cia_a_cra_sp_prev && !cia_a_cra_sp_now {
    keyboard.handshake();
}
cia_a_cra_sp_prev = cia_a_cra_sp_now;
```

This matches WinUAE's "old keyboard mode" handshake detection.

## State Machine

The keyboard controller progresses through a fixed sequence of states:

```
PowerUpDelay ──(timer expires)──▶ SendInitPowerUp
                                       │
                                  sends $FD
                                       ▼
                               WaitHandshakeInit ──(timeout)──▶ SendInitPowerUp
                                       │                        (resend $FD)
                                  (handshake)
                                       ▼
                               SendTermPowerUp
                                       │
                                  sends $FE
                                       ▼
                               WaitHandshakeTerm ──(timeout)──▶ SendTermPowerUp
                                       │                        (resend $FE)
                                  (handshake)
                                       ▼
                                     Idle ◀──────────────────── WaitHandshakeKey
                                       │                              ▲
                                  (key queued,                   (handshake
                                   byte interval                  or timeout)
                                   elapsed)                           │
                                       └── sends encoded key ────────┘
```

## Key Event Flow

Once idle, the keyboard sends queued key events:

1. User presses a key → `key_event(keycode, true)` queues the raw byte
2. After the byte interval (700 E-clock ticks) elapses, the keyboard sends
   the encoded byte
3. CIA-A SDR captures the byte, ICR bit 3 fires
4. Host reads SDR, decodes, and handshakes
5. Keyboard returns to idle, ready for the next key

Multiple keys can be queued simultaneously. They are sent in FIFO order, one
per handshake cycle.

## Interrupt Path

The full path from key event to software handler:

```
Keyboard sends byte → CIA-A SDR captures → ICR bit 3 (SP) set
  → If SP enabled in ICR mask: CIA-A IRQ asserts
  → Paula INTREQ bit 3 (PORTS) set
  → If PORTS enabled in INTENA + INTEN: IPL = level 2
  → CPU vectors to level 2 autovector
  → Software reads CIA-A ICR (clears all flags)
  → Software reads CIA-A SDR
  → Software decodes and processes keycode
  → Software handshakes (CRA bit 6 toggle)
```

## Common Keycodes

| Code | Key | Code | Key |
|------|-----|------|-----|
| $00 | ` (backtick) | $40 | Space |
| $01–$0A | 1–0 | $41 | Backspace |
| $10–$19 | Q–P | $42 | Tab |
| $20–$29 | A–L | $43 | Numpad Enter |
| $30–$38 | Z–. | $44 | Return |
| $45 | Escape | $46 | Delete |
| $50–$59 | F1–F10 | $60 | Left Shift |
| $61 | Right Shift | $62 | Caps Lock |
| $63 | Control | $64 | Left Alt |
| $65 | Right Alt | $66 | Left Amiga |
| $67 | Right Amiga | $4C–$4F | Cursor keys |

## Emulator Implications

- The keyboard is clocked at E-clock rate (~709 kHz), same as the CIAs. It
  ticks once per E-clock, not per CCK or per CPU cycle.
- The power-up delay (150K ticks ≈ 211ms) must not be skipped. KS expects
  to receive $FD and $FE during early boot. If the keyboard sends them too
  early (before KS enables the SP interrupt), they are missed — but the
  keyboard resends, so this is self-correcting.
- The encode function `!byte.rotate_left(1)` must match exactly. An earlier bug
  omitted the inversion, causing the ROM to decode $FE as keycode $01 ("1" key)
  instead of the power-up termination code.
- Handshake detection must use falling-edge detection on CRA bit 6, not level
  detection. Checking `CRA bit 6 == 0` would falsely trigger on every read of
  the idle state.
- Key-up events (bit 7 set) must be sent. Without them, the OS thinks keys
  are held down indefinitely — auto-repeat runs, modifier keys stick.
