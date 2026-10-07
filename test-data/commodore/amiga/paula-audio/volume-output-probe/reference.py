"""Compare pinned Paula PWM kernels; this is not a physical-chip oracle."""

import argparse
import csv
import gzip
import hashlib
import io
import json
import subprocess
from collections import defaultdict
from pathlib import Path

PINS = {
    "winuae": "c32694e338fa5f34977f522eb4898adb069d2e73",
    "minimig": "3ab91cd9220d4d047886d215b515227cbe568bdd",
}
VALUES = (0, 1, 31, 32, 63, 64, 127)
KEYS = {(0, v, 0) for v in range(128)} | {(1, v, p) for v in VALUES for p in range(64)}


def check(rows: list[list[int]]) -> dict[str, int]:
    """Check an exact inventory and HRM duty invariants, not a copied counter."""
    groups: dict[tuple[int, ...], list[list[int]]] = defaultdict(list)
    for row in rows:
        if len(row) != 8:
            raise ValueError("wrong column count")
        groups[tuple(row[:3])].append(row)
    if set(groups) != KEYS:
        raise ValueError("missing or unexpected scenarios")
    for (dynamic, value, phase), group in groups.items():
        if [r[3] for r in group] != list(range(64)):
            raise ValueError("missing or repeated clocks")
        for _, _, _, t, counter, volume, raw, gated in group:
            expected = value if not dynamic or t >= phase else 32
            if volume != expected or raw != 64 or not 0 <= counter < 64:
                raise ValueError("register, counter, or sample observation invalid")
            if gated not in (0, raw):
                raise ValueError("output is not a gated sample")
            if (volume == 0 and gated != 0) or (volume >= 64 and gated != raw):
                raise ValueError("zero/forced-maximum volume invariant violated")
        if not dynamic and sum(r[7] != 0 for r in group) != min(value, 64):
            raise ValueError("HRM 64-step duty invariant violated")
    return {"scenarios": len(groups), "observations": len(rows)}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--winuae", type=Path, required=True)
    parser.add_argument("--minimig", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    sources = {}
    for name, pin in PINS.items():
        root = getattr(args, name).resolve()
        revision = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
        ).strip()
        if revision != pin:
            raise ValueError(f"{name}: expected {pin}, got {revision}")
        relative = "audio.cpp" if name == "winuae" else "rtl/paula_audio_channel.v"
        path = root / relative
        # A matching HEAD does not establish an unchanged source file.
        original = subprocess.check_output(
            ["git", "-C", str(root), "show", f"{pin}:{relative}"]
        )
        if path.read_bytes() != original:
            raise ValueError(f"{name}: source differs from pinned revision")
        sources[name] = path
    subprocess.run(
        [
            "iverilog",
            "-g2012",
            "-s",
            "probe",
            "-o",
            str(out / "probe"),
            str(Path(__file__).with_name("probe.v")),
            str(sources["minimig"]),
        ],
        check=True,
    )
    trace = subprocess.check_output(["vvp", str(out / "probe")], text=True)
    # Icarus adds a $finish location line; never silently discard other output.
    lines = []
    for line in trace.splitlines():
        if "$finish called at" not in line:
            lines.append(line)
    rows = [[int(v) for v in row] for row in csv.reader(lines)]
    result = check(rows)
    rejected = []
    corrupt = [r.copy() for r in rows]
    corrupt[0][7] = 64
    for label, invalid in (("empty", []), ("corrupted output", corrupt)):
        try:
            check(invalid)
        except ValueError:
            rejected.append(label)
        else:
            raise AssertionError(f"checker accepted {label}")

    # Exact prefix of update_audio_volcnt, ending before its FIR/resampler.
    source = sources["winuae"].read_text()
    kernel = source[source.index("static void update_audio_volcnt(") :]
    kernel = kernel[: kernel.index("\tif (!nextsmp)")] + "}\n"
    harness = (
        source[: source.index("#include")]
        + r"""
#include <iostream>
#include <cassert>
#include <cstdint>
constexpr int AUDIO_CHANNELS_PAULA=1, CYCLE_UNIT=1, VOLCNT_BUFFER_SIZE=128;
union sIntFlt { uint32_t U32; float F32; };
struct audio_channel_data {
    struct { int new_sample=64, audvol=0; } data;
    int volcnt=0, volcntbufcnt=0;
    float volcntbuf[VOLCNT_BUFFER_SIZE]{};
} audio_channel[1];
"""
    )
    harness += kernel
    harness += r"""
int main() {
    int dynamic,value,phase,t,rtlcounter,volume,raw,gated;
    char comma;
    while(std::cin>>dynamic>>comma>>value>>comma>>phase>>comma>>t>>comma
          >>rtlcounter>>comma>>volume>>comma>>raw>>comma>>gated) {
        auto &ch=audio_channel[0];
        if(t==0) { ch={}; ch.volcnt=rtlcounter; }
        ch.data.audvol=volume>64?64:volume;
        int counter=ch.volcnt, index=ch.volcntbufcnt;
        update_audio_volcnt(1,0,false);
        int output=int(ch.volcntbuf[index]*128);
        assert(output==0 || output==64);
        std::cout<<dynamic<<','<<value<<','<<phase<<','<<t<<','<<counter<<','
                 <<volume<<','<<raw<<','<<output<<'\n';
    }
    if(!std::cin.eof()) return 1;
}
"""
    cpp = out / "winuae-kernel.cpp"
    cpp.write_text(harness)
    subprocess.run(
        [
            "c++",
            "-std=c++20",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-unused-parameter",
            str(cpp),
            "-o",
            str(out / "winuae-kernel"),
        ],
        check=True,
    )
    rtl_csv = "\n".join(lines) + "\n"
    uae_csv = subprocess.check_output(
        [str(out / "winuae-kernel")], input=rtl_csv, text=True
    )
    uae = [[int(v) for v in row] for row in csv.reader(io.StringIO(uae_csv))]
    if check(uae) != result:
        raise ValueError("reference inventories differ")
    result["gate_disagreements"] = sum(
        a[7] != b[7] for a, b in zip(rows, uae, strict=True)
    )
    # Compare each dynamic window with immediate scalar multiplication, in
    # sample units. This is an algebraic comparator, not native execution.
    for name, observations in (("minimig", rows), ("winuae", uae)):
        sums: dict[tuple[int, ...], int] = defaultdict(int)
        for row in observations:
            if row[0]:
                sums[tuple(row[:3])] += row[7] - min(row[5], 64)
        result[f"{name}_dynamic_windows_differing_from_scalar"] = sum(
            value != 0 for value in sums.values()
        )
    artifacts = {
        "minimig.csv.gz": rtl_csv.encode(),
        "winuae.csv.gz": uae_csv.encode(),
        "winuae-kernel.cpp.gz": harness.encode(),
    }
    for name, data in artifacts.items():
        (out / name).write_bytes(gzip.compress(data, mtime=0))
    report = {
        "inventory": result,
        "negative_controls_rejected": rejected,
        "pins": PINS,
        "source_sha256": {
            name: hashlib.sha256(path.read_bytes()).hexdigest()
            for name, path in sources.items()
        },
        "artifact_sha256": {
            name: hashlib.sha256((out / name).read_bytes()).hexdigest()
            for name in artifacts
        },
        "scope": "Manual constant sample, no DMA/attachment; synthetic shared initial "
        "counter for the WinUAE kernel; no startup/scheduler/FIR comparison.",
    }
    (out / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
