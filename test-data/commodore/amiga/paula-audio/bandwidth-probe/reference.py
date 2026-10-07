"""Execute the pinned WinUAE sinc kernels with explicit queue-time units."""

import argparse
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

PIN = "c32694e338fa5f34977f522eb4898adb069d2e73"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--winuae", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root, out = args.winuae.resolve(), args.output.resolve()
    out.mkdir(parents=True, exist_ok=True)
    revision = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    if revision != PIN:
        raise ValueError(f"unexpected WinUAE revision: {revision}")
    paths = ("audio.cpp", "sinctable.cpp", "include/sysdeps.h")
    for path in paths:
        committed = subprocess.check_output(
            ["git", "-C", str(root), "show", f"HEAD:{path}"]
        )
        if committed != (root / path).read_bytes():
            raise ValueError(f"modified reference: {path}")
    raw = (root / "audio.cpp").read_text()
    preamble = raw[: raw.index('#include "sysconfig.h"')]
    structs = raw[
        raw.index(
            "typedef struct {", raw.index("#define SINC_QUEUE_MAX_AGE")
        ) : raw.index("struct audio_stream_data")
    ]
    methods = raw[
        raw.index("static void sinc_prehandler_paula (") : raw.index(
            "static void do_filter("
        )
    ]
    table = (root / "sinctable.cpp").read_text()
    source = (
        preamble
        + r"""
#include <cmath>
#include <iomanip>
#include <iostream>
#include <vector>
using uae_u8 = unsigned char;
#define SINC_QUEUE_MAX_AGE 2048
#define SINC_QUEUE_LENGTH 256
#define AUDIO_CHANNELS_PAULA 4
constexpr int FILTER_MODEL_A500=1, FILTER_MODEL_A500_FIXEDONLY=2;
int sound_use_filter_sinc=0, led_filter_on=0;
"""
        + table
        + structs
        + r"""
audio_channel_data2 channels[4];
audio_channel_data2 *audio_data[4] = {channels,channels+1,channels+2,channels+3};
"""
        + methods
        + r"""
int main() {
    std::cout << std::setprecision(12);
    for (int units : {1,512}) for (int frequency : {1000,20000,28000,55000,95000}) {
        for (auto &c:channels) { c={}; c.mixvol=1; c.adk_mask=~0u; }
        long phase=0; int frames=0, data[4]{};
        std::vector<double> samples;
        // Keep edges eight CCKs apart, within the kernel's documented
        // 256-event queue capacity. This is a synthetic held sine, not DMA.
        for (long tick=0;frames<7200;++tick) {
            if (tick%8==0) channels[0].current_sample=int(std::round(
                4096*std::sin(2*3.14159265358979323846*frequency*tick/3546895.0)));
            sinc_prehandler_paula(units);
            phase+=48000;
            if (phase>=3546895) {
                phase-=3546895;
                samplexx_sinc_handler(data,0,4);
                if (frames>=2400) samples.push_back(data[0]/16384.0);
                ++frames;
            }
        }
        const int alias=std::min(frequency%48000,48000-frequency%48000);
        double re=0,im=0,wtotal=0;
        for (unsigned n=0;n<samples.size();++n) {
            const double w=0.5-0.5*std::cos(2*3.14159265358979323846*n/samples.size());
            const double a=2*3.14159265358979323846*alias*n/48000.0;
            re+=samples[n]*w*std::cos(a); im+=samples[n]*w*std::sin(a); wtotal+=w;
        }
        std::cout << units << ',' << frequency << ',' << alias << ',' << 2*std::hypot(re,im)/wtotal << '\n';
    }
}
"""
    )
    cpp = out / "winuae.cpp"
    cpp.write_text(source)
    subprocess.run(
        [
            "c++",
            "-std=c++20",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            str(cpp),
            "-o",
            str(out / "winuae"),
        ],
        check=True,
    )
    observations = subprocess.check_output([str(out / "winuae")], text=True)

    def check(text: str) -> None:
        rows = [line.split(",") for line in text.splitlines()]
        expected = [
            (u, f) for u in (1, 512) for f in (1000, 20000, 28000, 55000, 95000)
        ]
        if [(int(row[0]), int(row[1])) for row in rows] != expected:
            raise ValueError("incomplete reference inventory")
        for row in rows:
            if not 0 <= float(row[3]) <= 1.1:
                raise ValueError("invalid measured gain")
        if float(rows[0][3]) < 0.9 or float(rows[5][3]) < 0.9:
            raise ValueError("silent fixture")

    check(observations)
    for invalid in (
        "",
        observations.replace(observations.splitlines()[0], "1,1000,1000,nan", 1),
    ):
        try:
            check(invalid)
        except ValueError:
            continue
        raise AssertionError("reference validator accepted invalid data")
    (out / "winuae.csv").write_text(observations)
    (out / "winuae.cpp.gz").write_bytes(gzip.compress(source.encode(), mtime=0))
    report = {
        "revision": revision,
        "source_sha256": {
            path: hashlib.sha256((root / path).read_bytes()).hexdigest()
            for path in paths
        },
        "observations": 10,
        "negative_controls": ["empty", "non-finite amplitude"],
        "units": {
            "1": "deliberately normalised CCK queue age; exploratory, not the pinned caller",
            "512": "CYCLE_UNIT from pinned sysdeps.h and update_audio best_evtime call",
        },
        "limitations": "Isolated kernel, PAL, vanilla table, no board filter, 8-CCK held synthetic sine; not a live WinUAE or physical hardware capture. Do not silently normalise caller units into an oracle.",
    }
    (out / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
    print(observations, end="")


if __name__ == "__main__":
    main()
