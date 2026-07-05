#!/usr/bin/env python3
"""Drive VICE's binary monitor to cross-check our cores against a reference.

Use case: when one of our machine/drive cores diverges from real hardware,
VICE (x64sc, x128, xvic, …) is the cycle-accurate oracle. This talks to its
*binary* monitor so you can, from a script, read the CPU registers and memory
of any CPU in the machine (the main CPU *and* each true-drive's 6502), set
execution checkpoints, and dump chosen state every time one is hit. That is
exactly how the C64->1581 serial LOAD bug (#69) was cracked: a checkpoint on
the drive's ATN poll showed VICE reading PB7=0 where we read 1 — an inverted
IEC input line.

This is the cross-emulator sibling of `fs-uae-cross-check.sh` (Amiga) and
`chipset-read-log-diff.py`.

--------------------------------------------------------------------------
Launching VICE so the monitor actually works
--------------------------------------------------------------------------
The binary monitor only services commands while VICE's main loop is
emulating, so VICE MUST be launched ATTACHED to its GUI — no `nohup`, no
`&`-into-a-dead-terminal, and NOT `-warp` (warp starves the monitor). Use
the *binary* monitor; the text `-remotemonitor` does not speak this protocol.

    x64sc +sound -default \
        -drive8type 1581 -drive8truedrive \
        -8 /path/to/disk.d81 \
        -binarymonitor -binarymonitoraddress ip4://127.0.0.1:6502

Force true drive emulation (`-drive8truedrive`) or the drive CPU isn't
emulated and its memspace is empty. `-autostart-handle-tde` + `-autostart
<disk>` if you want VICE to type the LOAD for you.

--------------------------------------------------------------------------
Memspaces
--------------------------------------------------------------------------
    main / c64   0     drive8   1     drive9   2     drive10  3     drive11  4

--------------------------------------------------------------------------
Examples
--------------------------------------------------------------------------
    # Where is drive 8's CPU right now, and what is its zero-page $76?
    vice-monitor.py regs --space drive8
    vice-monitor.py mem 0x76 --space drive8

    # Compare an IEC input line: read the drive CIA Port B ($4001).
    vice-monitor.py mem 0x4001 --space drive8

    # Reset, then log every time drive 8 executes its ATN poll ($AD21) or
    # reaches its idle ($B105), dumping $76 and the CIA Port B each time,
    # stopping once it reaches $B105.
    vice-monitor.py trace 0xAD21 0xB105 --space drive8 --reset \
        --read 0x76 0x4001 --until 0xB105

Protocol reference: VICE manual, "Binary monitor". Only stdlib is used.
"""

import argparse
import socket
import struct
import sys

DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = 6502

# Command types.
CMD_MEM_GET = 0x01
CMD_MEM_SET = 0x02
CMD_CHECKPOINT_SET = 0x12
CMD_CHECKPOINT_DELETE = 0x13
CMD_CHECKPOINT_LIST = 0x14
CMD_REGISTERS_GET = 0x31
CMD_REGISTERS_AVAILABLE = 0x83
CMD_ADVANCE = 0x71
CMD_EXIT = 0xAA
CMD_RESET = 0xCC
CMD_PING = 0x81

# Response / async-event types.
RESP_CHECKPOINT_INFO = 0x11

# CPU operation flags for a checkpoint.
OP_LOAD = 1
OP_STORE = 2
OP_EXEC = 4

MEMSPACES = {
    "main": 0,
    "c64": 0,
    "cpu": 0,
    "drive8": 1,
    "drive9": 2,
    "drive10": 3,
    "drive11": 4,
}


def parse_addr(text):
    """Accept ``$AD21``, ``0xAD21``, ``AD21`` or a decimal literal."""
    text = text.strip()
    if text.startswith("$"):
        return int(text[1:], 16)
    if text.lower().startswith("0x"):
        return int(text, 16)
    try:
        return int(text, 16)
    except ValueError:
        return int(text, 10)


class ViceMonitor:
    """A thin, synchronous client for VICE's binary monitor."""

    def __init__(self, host=DEFAULT_HOST, port=DEFAULT_PORT, timeout=30):
        self.sock = socket.create_connection((host, port), timeout=timeout)
        self.sock.settimeout(timeout)
        self._buf = b""
        self._reqid = 0x1000
        self._pending = []  # async frames read while awaiting a response
        self._reg_names = {}

    # -- framing ----------------------------------------------------------
    def _recv_exact(self, n):
        while len(self._buf) < n:
            chunk = self.sock.recv(65536)
            if not chunk:
                raise EOFError("VICE monitor closed the connection")
            self._buf += chunk
        out, self._buf = self._buf[:n], self._buf[n:]
        return out

    def _read_frame(self):
        # header: STX(02) API(02) len(u32) type(1) err(1) reqid(u32) = 12 bytes
        hdr = self._recv_exact(12)
        if hdr[0] != 0x02 or hdr[1] != 0x02:
            raise ValueError(f"bad response magic {hdr[:2].hex()}")
        (blen,) = struct.unpack_from("<I", hdr, 2)
        rtype, err = hdr[6], hdr[7]
        (rid,) = struct.unpack_from("<I", hdr, 8)
        return rtype, err, rid, self._recv_exact(blen)

    def _command(self, ctype, body=b""):
        self._reqid += 1
        rid = self._reqid
        pkt = (
            bytes([0x02, 0x02])
            + struct.pack("<I", len(body))
            + struct.pack("<I", rid)
            + bytes([ctype])
            + body
        )
        self.sock.sendall(pkt)
        while True:
            rtype, err, rrid, rbody = self._read_frame()
            if rrid == rid:
                return rtype, err, rbody
            self._pending.append((rtype, err, rrid, rbody))

    # -- high-level ops ---------------------------------------------------
    def ping(self):
        rtype, err, _ = self._command(CMD_PING)
        return rtype == CMD_PING and err == 0

    def register_names(self, space):
        _, _, body = self._command(CMD_REGISTERS_AVAILABLE, bytes([space]))
        (count,) = struct.unpack_from("<H", body, 0)
        off, names = 2, {}
        for _ in range(count):
            itemlen = body[off]
            rid = body[off + 1]
            nlen = body[off + 3]
            names[rid] = body[off + 4 : off + 4 + nlen].decode("ascii", "replace")
            off += 1 + itemlen
        return names

    def registers(self, space):
        names = self._reg_names.get(space)
        if names is None:
            names = self._reg_names[space] = self.register_names(space)
        _, _, body = self._command(CMD_REGISTERS_GET, bytes([space]))
        (count,) = struct.unpack_from("<H", body, 0)
        off, regs = 2, {}
        for _ in range(count):
            itemlen = body[off]
            rid = body[off + 1]
            (val,) = struct.unpack_from("<H", body, off + 2)
            regs[names.get(rid, str(rid))] = val
            off += 1 + itemlen
        return regs

    def mem(self, start, end, space):
        body = (
            bytes([0])
            + struct.pack("<H", start)
            + struct.pack("<H", end)
            + bytes([space])
            + struct.pack("<H", 0)
        )
        _, _, rbody = self._command(CMD_MEM_GET, body)
        (mlen,) = struct.unpack_from("<H", rbody, 0)
        return rbody[2 : 2 + mlen]

    def mem_set(self, start, values, space):
        end = start + len(values) - 1
        body = (
            bytes([0])
            + struct.pack("<H", start)
            + struct.pack("<H", end)
            + bytes([space])
            + struct.pack("<H", 0)
            + bytes(values)
        )
        self._command(CMD_MEM_SET, body)

    def checkpoint_set(self, addr, space, op=OP_EXEC):
        body = (
            struct.pack("<H", addr)
            + struct.pack("<H", addr)
            + bytes([1, 1, op, 0, space])  # stop, enabled, op, temporary, memspace
        )
        _, _, rbody = self._command(CMD_CHECKPOINT_SET, body)
        (num,) = struct.unpack_from("<I", rbody, 0)
        return num

    def reset(self, hard=True):
        self._command(CMD_RESET, bytes([1 if hard else 0]))

    def resume(self):
        # EXIT leaves the monitor and resumes emulation. Fire-and-forget; the
        # ack (and the async RESUMED) are skipped by the next wait.
        self._reqid += 1
        pkt = (
            bytes([0x02, 0x02])
            + struct.pack("<I", 0)
            + struct.pack("<I", self._reqid)
            + bytes([CMD_EXIT])
        )
        self.sock.sendall(pkt)

    def close(self):
        try:
            self.sock.close()
        except OSError:
            pass

    def wait_checkpoint(self):
        """Block until a checkpoint fires; return its start address."""
        for frame in list(self._pending):
            if frame[0] == RESP_CHECKPOINT_INFO:
                self._pending.remove(frame)
                return struct.unpack_from("<H", frame[3], 5)[0]
        while True:
            rtype, _, _, body = self._read_frame()
            if rtype == RESP_CHECKPOINT_INFO:
                return struct.unpack_from("<H", body, 5)[0]


# -- subcommands ----------------------------------------------------------
def _space(args):
    return MEMSPACES[args.space]


def cmd_ping(mon, args):
    print("ok" if mon.ping() else "no response")


def cmd_regs(mon, args):
    regs = mon.registers(_space(args))
    order = ["PC", "A", "X", "Y", "SP", "FL"]
    shown = [f"{k}=${regs[k]:0{4 if k == 'PC' else 2}X}" for k in order if k in regs]
    extra = [f"{k}=${v:X}" for k, v in regs.items() if k not in order]
    print(f"[{args.space}] " + "  ".join(shown + extra))


def cmd_mem(mon, args):
    start = parse_addr(args.start)
    end = parse_addr(args.end) if args.end else start
    data = mon.mem(start, end, _space(args))
    for row in range(0, len(data), 16):
        chunk = data[row : row + 16]
        hexs = " ".join(f"{b:02X}" for b in chunk)
        print(f"[{args.space}] ${start + row:04X}: {hexs}")


def cmd_set(mon, args):
    start = parse_addr(args.start)
    values = [parse_addr(v) & 0xFF for v in args.values]
    mon.mem_set(start, values, _space(args))
    print(f"[{args.space}] wrote {len(values)} byte(s) at ${start:04X}")


def cmd_reset(mon, args):
    mon.reset(hard=not args.soft)
    print("reset (%s)" % ("soft" if args.soft else "hard"))


def cmd_trace(mon, args):
    space = _space(args)
    targets = {parse_addr(a): a for a in args.addr}
    reads = [parse_addr(a) for a in (args.read or [])]
    until = parse_addr(args.until) if args.until else None
    for addr in targets:
        mon.checkpoint_set(addr, space, OP_EXEC)
    if args.reset:
        mon.reset(hard=True)
        print("-- reset; tracing --")
    for _ in range(args.max):
        mon.resume()
        hit = mon.wait_checkpoint()
        regs = mon.registers(space)
        cells = " ".join(
            f"${a:04X}={mon.mem(a, a, space)[0]:02X}" for a in reads
        )
        iflag = (regs.get("FL", 0) >> 2) & 1
        label = targets.get(hit, f"${hit:04X}")
        print(
            f"  hit {label:>7}  PC=${regs.get('PC', 0):04X}  I={iflag}"
            + (f"  {cells}" if cells else "")
        )
        if until is not None and hit == until:
            print(f"  -> reached ${until:04X}")
            break


def build_parser():
    p = argparse.ArgumentParser(
        description="Cross-check our cores against VICE via its binary monitor.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="See the module docstring for the VICE launch recipe.",
    )
    p.add_argument("--host", default=DEFAULT_HOST)
    p.add_argument("--port", type=int, default=DEFAULT_PORT)
    p.add_argument(
        "--space",
        default="c64",
        choices=sorted(MEMSPACES),
        help="which CPU's address space (default: c64)",
    )
    sub = p.add_subparsers(dest="cmd", required=True)

    sub.add_parser("ping", help="check the monitor is responding").set_defaults(
        func=cmd_ping
    )
    sub.add_parser("regs", help="dump CPU registers").set_defaults(func=cmd_regs)

    pm = sub.add_parser("mem", help="read memory")
    pm.add_argument("start")
    pm.add_argument("end", nargs="?")
    pm.set_defaults(func=cmd_mem)

    ps = sub.add_parser("set", help="write memory")
    ps.add_argument("start")
    ps.add_argument("values", nargs="+", help="byte value(s)")
    ps.set_defaults(func=cmd_set)

    pr = sub.add_parser("reset", help="reset the machine")
    pr.add_argument("--soft", action="store_true", help="soft reset (default hard)")
    pr.set_defaults(func=cmd_reset)

    pt = sub.add_parser("trace", help="log execution-checkpoint hits + state")
    pt.add_argument("addr", nargs="+", help="checkpoint address(es)")
    pt.add_argument("--reset", action="store_true", help="hard-reset before tracing")
    pt.add_argument("--read", nargs="+", metavar="ADDR", help="dump these on each hit")
    pt.add_argument("--until", metavar="ADDR", help="stop once this address is hit")
    pt.add_argument("--max", type=int, default=40, help="max hits to log (default 40)")
    pt.set_defaults(func=cmd_trace)
    return p


def main(argv=None):
    args = build_parser().parse_args(argv)
    try:
        mon = ViceMonitor(args.host, args.port)
    except OSError as exc:
        sys.exit(
            f"cannot reach VICE binary monitor at {args.host}:{args.port} ({exc}).\n"
            "Is VICE running with -binarymonitor, attached to its GUI (no -warp)?"
        )
    try:
        args.func(mon, args)
    finally:
        mon.close()


if __name__ == "__main__":
    main()
