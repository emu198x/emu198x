# Stage AE — AGA WB-doesn't-install-view: handoff

Session of 2026-05-27 worked through the AGA WB-install bug from the
*outside in*: corrected chipset identification (AE-j), added ECS
blitter extensions (AE-k), restored the real AGA Alice agnus_id
(AE-l), built three new MCP inspection tools (AE-m / AE-o + the
`tools/chipset-read-log-diff.py` helper from AE-n), and narrowed the
blocker to a specific upstream task. We've identified *what* is
stuck; we don't yet know *why*.

This handoff captures the state so the next session can pick up
without re-deriving 16 stages of context.

## Where the bug is now (post Stage AE-o)

A1200 + KS 3.1 + WB 3.0/3.1 boots through KS but **Workbench never
draws its desktop**. The framebuffer shows KS's "Insert Workbench
Disk" prompt with KS's diagnostic palette ($0F00 / $00F0 / $0FF0 at
COLOR1/2/3) instead of WB's grey/blue.

Concrete current-state symptoms (verified by running the AE-m / AE-o
MCP tools against the boot at frame ~3000 post-disk-insert):

- `cop2lc = $121E0` and stays there across 600+ frames. KS's VBL
  handler at PC `$F8F7A2` rewrites it every frame; WB never
  installs its own copper list (17,866 cop2lc writes recorded, all
  to the KS-default address).
- Disk wedges at cylinder 40 with motor off. Same chip RAM size /
  fast RAM tested.
- CPU is mostly idle: ~1,271 instructions per frame, hot loop is
  Exec's SoftIntDispatch at `$F817B4`.
- 9 tasks in TaskWait, 0 in TaskReady. ThisTask = `input.device`,
  just woke from a softint.

## What is true now (chipset side)

- VPOSR upper-byte (Agnus ID) per model — verified via probe:
  | model     | VPOSR & $7F00 | identification |
  |-----------|---------------|----------------|
  | a500      | `$1000`       | OCS PAL 8367   |
  | a500-plus | `$2000`       | ECS PAL 8375   |
  | a600      | `$2000`       | ECS PAL 8375   |
  | a1200     | `$2300`       | AGA Alice PAL  |
- DENISEID per chipset — `$FFFF` OCS / `$FFFC` ECS / `$FFF8` AGA,
  all reaching KS through `dispatch_custom_register`'s read arm.
- FMODE ($1FC) read-back wired on the AGA machine; KS 3.1 reads it
  during boot and gets the value it wrote (default 0).
- ECS blitter extension registers ($05A BLTCON0L, $05C BLTSIZV,
  $05E BLTSIZH) handled correctly by both ECS and AGA machines
  (Stage AE-k).
- HIRES rendering verified pixel-perfect via the bitmap-poke test
  (Stage AB hypothesis), still holds.

## What we've identified (the upstream blocker)

The `query_exec_tasks` MCP tool (Stage AE-m) walking the wedged
boot at frame ~3000 shows 9 waiting tasks. Cross-referencing against
a working A500+ + KS 2.04 + WB 2.1 boot reveals the smoking gun:

| Task        | ECS sig_alloc / sig_wait     | AGA sig_alloc / sig_wait    |
|-------------|------------------------------|------------------------------|
| **IPrefs**  | `$C000FFFF` / `$C0009000`    | `$0000FFFF` / `$0000F000`    |
| Workbench   | `$8000FFFF` / `$C0000000`    | `$8000FFFF` / `$80000000`    |
| trackdisk   | `$0000FFFF` / `$00000300`    | `$0000FFFF` / `$00000300`    |
| ConClip     | `$8001FFFF` / `$80001000`    | `$8000FFFF` / `$80000000`    |

**IPrefs is the smoking gun**: on ECS it has signals 30+31 allocated
for its message ports (`sig_alloc=$C000FFFF`). On AGA it has only
bits 0-15 allocated (`sig_alloc=$0000FFFF`) — **IPrefs hasn't even
reached the `AllocSignal()` calls that set up its message ports**.

IPrefs's `tc_sp_reg` points to `$2D612` in its task stack. The top
of stack is `$00F80C0C` (KS ROM — the Wait()-return point in the
exec scheduler). The wait mask is `$0000F000` = SIGBREAK bits 12-15
only. So IPrefs is in some early-init `Wait(SIGBREAKF_*)` call
that's never being satisfied — the call should normally proceed
past this point, allocate ports, then sit in steady-state on those.

WB downstream waits for IPrefs (via signal bit 31 on one of WB's 5
private MsgPorts located by chip-RAM scan in AE-o); IPrefs never
sends it.

## What we've ruled out

- **HIRES rendering** — poke test pixel-perfect (the original Stage
  AE hypothesis, disproved in Stage AB).
- **Chip RAM size** — 512 KB / 2 MB / 2 MB + 4 MB fast all behave
  identically.
- **Disk-loading differential** — disk wedges at cylinder 40 / motor
  off on both the working (AB) and broken (AE) paths.
- **Chipset register reads** — DMACONR, INTENAR, INTREQR, VPOSR,
  VHPOSR, POTGOR, JOY0DAT, DENISEID, FMODE all return values KS
  expects. Verified via `chipset_read_log`.
- **ECS blitter extension registers** — added in AE-k, exercised by
  KS 2.04+ code paths. Not relevant to the AGA wedge (the OCS-
  impersonation pre-AE-l fallback used the same BLTSIZE path).
- **WB version** — WB 3.0 and WB 3.1 wedge identically; not a
  WB-binary issue, it's a KS-3.x-on-AGA issue.
- **trackdisk signal mismatch** (sig_wait=$300, sig_recvd=$400) —
  exists on both ECS and AGA. Same behaviour, not the differentiator.
- **Public message ports** — `query_exec_ports` shows PortList has
  only one entry (ConClip.rendezvous). All of WB's and IPrefs's
  ports are private (anonymous, allocated directly without `AddPort`).
- **chipset_read_log diff (AE-n)** — AGA reads `$1FC FMODE` (AGA-
  only, handled correctly); no other AGA-only chipset reads. INTENAR
  / INTREQR value distributions differ between AGA and ECS but
  that's secondary to the IPrefs blocker.

## What's left

### Immediate — pinpoint what IPrefs's Wait() is for

IPrefs is parked at `Wait($0000F000)` from a chip-RAM call site
~`$2D69C` (best guess from stack walk). To make progress we need to
identify the *function* that called this Wait.

Three concrete diagnostic paths, in order of cheapest to most
expensive:

1. **Decode the Process struct.** IPrefs is `NT_PROCESS` (type 13),
   which extends Task with `pr_MsgPort` (embedded), `pr_SegList`,
   `pr_CIS/COS/CES`, `pr_CurrentDir`, `pr_HomeDir`, etc. The
   embedded `pr_MsgPort` would tell us what bit-number was supposed
   to be allocated for it (probably bit 8 — the standard DOS Process
   port). Reading `pr_CurrentDir` / `pr_HomeDir` would tell us where
   IPrefs's working directory is set (whether `SYS:` is mounted,
   whether prefs files are reachable). Tool to add: extend
   `query_exec_tasks` to detect `ln_Type == 13` and decode Process
   fields.

2. **LVO-aware function resolution.** Every Amiga library call goes
   through `jsr -N(a6)` where -N is the LVO offset. With a lookup
   table of well-known LVOs (exec is the most important: -300 is
   Wait, -316 is WaitPort, -384 is Signal, -636 is AllocSignal), we
   can identify any KS function by its address. Concretely: KS PC
   `$00F80C0C` is the return point of a specific exec function —
   knowing which one would tell us if IPrefs called Wait, WaitPort,
   or something else.

3. **Cross-emulator comparison.** Boot the same KS 3.1 + WB 3.1 in
   vAmiga (or fs-uae with debug logging) and capture either a
   library-call trace or an `IPrefs` task-state dump at the same
   frame number. Compare against ours to find the exact call site
   that diverges. Most diagnostically powerful but heaviest setup.

### Medium-term — restore WB rendering on AGA

Once we know what IPrefs is waiting for and which subsystem should
signal it, the fix is one of:

- An interrupt or signal we're not generating (CIA timer event,
  DSK / serial / other Paula interrupt, AGA-specific event)
- A library function we don't implement (intuition.library
  `OpenScreenTags` with an AGA-specific tag, graphics.library
  `MakeVPort` with HAM8 or AGA palette)
- A bus / memory behaviour we model incorrectly (FMODE-dependent
  bitplane fetch, copper-driven palette banking)

### Long-term — reusable Amiga-OS-aware tooling

The investigation tools added this session (`query_exec_tasks`,
`query_exec_ports`, `chipset_read_log` diff helper) are
chipset-agnostic — they apply equally well to OCS / ECS / AGA. The
next round of tools (Process decoder, LVO resolver, library-call
trace) are also chipset-agnostic and immediately useful for any
future Amiga debugging. These belong in the family runtime, not
the AGA-specific code path.

## Tools available now (post-AE-o)

Chipset / chip-level (cross-chipset):
- `query_cpu`, `query_chipset`, `query_paula`, `query_cia`,
  `query_agnus`, `query_blitter`, `query_aga` (Lisa-specific)
- `chipset_write_log` (AE-h), `chipset_read_log`, `palette_log`,
  `bplcon0_log`, `watch_memory`
- `cpu_trace_arm` / `cpu_trace_log` (AE-i)
- `memory_read`, `memory_read_long`, `poke_word`, `poke_byte`
- `disasm`, `step`, `run_until_pc`, `run_until_any_pc`,
  `run_until_mem_change`, `run_frames`, `run_ticks`
- `dump_framebuffer`, `start/stop_video_recording`,
  `insert_media`, `eject_media`, `query_disk`

Amiga-OS-aware (new this session):
- `query_exec_tasks` (AE-m) — walks ExecBase / ThisTask /
  TaskReady / TaskWait, decodes Node + Task struct fields
- `query_exec_ports` (AE-o) — walks ExecBase->PortList, decodes
  MsgPort struct including queued message count

External helper:
- `tools/chipset-read-log-diff.py` (AE-n) — diffs two MCP
  chipset_read_log captures and surfaces register-access divergence

Recommended next tools (queued as AE-q / AE-r / AE-s):
- `memory_scan` — find all addresses where a 32-bit value appears
  in a memory range (generalises the chip-RAM scan that found WB's
  private MsgPorts)
- `resolve_lvo` + library-base discovery — given an address, name
  the function (e.g. `$F80C0C` → "exec.library/Wait return point")
- Process struct decoder — extend `query_exec_tasks` to decode
  Process-specific fields when `ln_Type == 13` (NT_PROCESS)

## How to reproduce + verify

Drive the wedge directly:

```sh
DISK="$HOME/.emu198x/media/commodore-amiga/wb31/Workbench v3.1 rev 40.42 (1996)(ESCOM)(M10)(Disk 2 of 6)(Workbench).adf"
printf '%s\n%s\n%s\n%s\n%s\n%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":240}}}' \
  "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"insert_media\",\"arguments\":{\"path\":\"$DISK\"}}}" \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"run_frames","arguments":{"frames":3000}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"query_exec_tasks","arguments":{}}}' \
  '{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"dump_framebuffer","arguments":{"path":"/tmp/wedge.png"}}}' \
  | ./target/release/emu198x-amiga --mcp --model a1200
```

Look for: IPrefs task with `sig_alloc=$0000FFFF` (no port signals
allocated), Workbench task with `sig_wait=$80000000` (waiting on its
main port), framebuffer showing the diagnostic-palette KS prompt.

ECS comparison (working baseline):

```sh
WB21="$HOME/Projects/198x/assets/amiga/Operating Systems/Workbench/Workbench v2.1 rev 38.35 (1992)(Commodore)(M10)(Disk 2 of 5)(Workbench)[Cloanto Amiga Forever Edition].zip"
# substitute the disk path; otherwise identical to above with --model a500-plus
```

Should produce: IPrefs with `sig_alloc=$C000FFFF`, Workbench with
`sig_recvd=$0100`, full WB 2.1 desktop framebuffer.

## Recent commits (AE-j → AE-o)

```
173c087  AE-o  query_exec_ports — public MsgPort inspector + WB-port scan
3b0ab74  AE-n  chipset_read_log diff helper (tools/chipset-read-log-diff.py)
58c6383  AE-m  query_exec_tasks — ExecBase / task-list inspector
b016d54  AE-l  restore real AGA Alice agnus_id ($2300 PAL / $3300 NTSC)
d64026b  AE-k  ECS blitter extension registers (BLTCON0L / BLTSIZV / BLTSIZH)
983de7f  AE-j  correct chipset identification across OCS / ECS / AGA
c8109d2  AE-h+i investigation tooling — chipset write log + CPU instruction trace
e20f5f9  AE-g  --model CLI flag for the MCP server
98d3609  AE-f  rename AmigaA1200Session → AmigaSession
9962097  AE-e  mirror BPLCON0 / palette / chipset-read tracers onto OCS + ECS
6d720bd  AE-d  lift more MCP tools off the A1200 downcast
3a18978  AE-c  route cross-cutting MCP tools through AmigaLiveAccess
d53b8a9  AE-b  AmigaLiveAccess trait — chipset-agnostic chip access
```
