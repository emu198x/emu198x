# Simulating the real ULA Verilog

`hdl_model.rs` is a *transcription* of the contention block of
`opencores.org/projects/zx_ula`, and a transcription is only a reading.
These testbenches check that reading against the actual gates, by running
the Verilog under Icarus Verilog.

They are not part of `cargo test` — they need `iverilog` and the vendored
HDL, neither of which the workspace depends on. Run them by hand when the
transcription changes, or when a conclusion rests on it.

```sh
brew install icarus-verilog

# A checkout of the zx_ula FPGA project. Vendored under
# emulators/zx-spectrum/zx_ula in the 198x tree; clone it yourself otherwise.
ZX_ULA=/path/to/zx_ula

# Run this from the directory holding this README.
TB="$PWD/tb_window.v"

cd "$ZX_ULA"
iverilog -g2005 -o /tmp/tb.vvp "$TB" fpga_version/rtl/ula.v
vvp /tmp/tb.vvp
```

## `tb_window.v` — where the contention window opens

Holds `a = $4000` (A14 high, A15 low, so the memory decode is satisfied)
with `/MREQ` inactive, so `mreqt23` stays high and `Nor1` reduces to the
raster terms alone. Whatever stalls `CPUClk` is then the window and nothing
else. One T-state is four `clk14`, because `CPUClk = clk7/2 = clk14/4`.

Measured: the first stall is at **T-state 14338** after `/INT`, then every
8. FUSE contends from **14335**. That three-T-state displacement is the
central open question in
`knowledge/decisions/spectrum-contention-vs-floating-bus.md`, and this is
what establishes it as real silicon behaviour rather than a misreading.

Address decode, same bench with the held values changed:

| `a` | `/MREQ` | `/IORQ` | result |
|---|---|---|---|
| `$4000` | inactive | inactive | stalls every 8 T-states |
| `$C000` | inactive | inactive | no stalls |

## `tb_io.v` — does a ULA port contend outside the contended page?

The claim that resolved the Chapter 18 prose fork is that `Nor1`'s `IORQ`
term satisfies both address conditions on its own, so a port the ULA
answers contends whatever page its address lands in.

It cannot be tested with a statically asserted `/IORQ`: `ioreqtw3` latches
low on the first `CPUClk` rise and disarms the gate permanently — which is
the cancellation working, not the absence of the path. So this pulses
`/IORQ` low for five `clk7`, the span a Z80 I/O cycle holds it across `T2`,
`TW` and `T3`.

**Read the result carefully.** Sweeping the pulse phase, `$C0FE` produces
exactly *one* stall at even phases 8, 12, 16, 20, 24, 28 and none
elsewhere; `$C0FF` never stalls; `$40FE` and `$40FF` stall throughout on
the memory decode. The single stall is the point: the pulse period is
exactly eight `CPUClk` periods, so the alignment holds only until the first
stall stretches the clock and destroys it. The path fires; this stimulus
cannot sustain the phase to fire it again.

That is enough to confirm the path exists and is even-port-only, and **not**
enough to establish the per-class costs. Settling those needs a testbench
driving realistic Z80 M-cycles — `fpga_version/ula_test_for_ise_and_isim/
cpu.v` is a starting point — with the CPU clocked by `clkcpu` so stalls
feed back the way they do in hardware.
