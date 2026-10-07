"""Execute pinned reference amplitude paths at forced-full volume."""

import argparse
import gzip
import hashlib
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--winuae", type=Path, required=True)
parser.add_argument("--vamiga", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
subprocess.run(
    [
        "python3.13",
        str(Path(__file__).resolve().parent.parent / "handover-probe/reference.py"),
        "--winuae",
        str(args.winuae.resolve()),
        "--vamiga",
        str(args.vamiga.resolve()),
        "--output",
        str(out / "base"),
    ],
    check=True,
)
uae = (
    (out / "base/winuae.cpp")
    .read_text()
    .split("void run(unsigned nr,unsigned p,unsigned scenario")[0]
)
macro = next(
    line
    for line in (args.winuae / "audio.cpp").read_text().splitlines()
    if line.startswith("#define DO_CHANNEL_1(")
)
uae += (
    macro
    + r"""
int main() {
    for(unsigned nr=0;nr<4;++nr) for(unsigned volume:{64,127}) for(unsigned byte=0;byte<256;++byte) {
        for(auto &ch:audio_channel) ch=audio_channel_data{};
        for(auto &due:irq_due) due=MAX_EV;
        now=intreq=adkcon=dmacon=0;
        update_volume(nr,volume);
        dat_write(nr,byte*0x101);
        int scaled=audio_channel[nr].data.current_sample;
        DO_CHANNEL_1(scaled,nr);
        std::cout<<nr<<','<<volume<<','<<byte<<','<<scaled<<'\n';
    }
}
"""
)
vamiga = (
    (out / "base/vamiga.cpp")
    .read_text()
    .split("template<isize nr> void run(unsigned period,unsigned scenario")[0]
)
vamiga += r"""
template<isize nr> void run() {
    for(unsigned volume:{64,127}) for(unsigned byte=0;byte<256;++byte) {
        Paula p; auto &ch=channel<nr>(p);
        ch.pokeAUDxVOL(volume); ch.pokeAUDxDAT(byte*0x101);
        std::cout<<nr<<','<<volume<<','<<byte<<','<<ch.audioPort.sampler[nr].last<<'\n';
    }
}
int main() { run<0>(); run<1>(); run<2>(); run<3>(); }
"""
expected = "".join(
    f"{nr},{volume},{byte},{(byte if byte < 128 else byte - 256) * 64}\n"
    for nr in range(4)
    for volume in (64, 127)
    for byte in range(256)
)


def check(observations: str) -> None:
    if observations != expected:
        raise ValueError("reference amplitude or inventory mismatch")


for name, source in (("winuae", uae), ("vamiga", vamiga)):
    cpp = out / f"{name}.cpp"
    cpp.write_text(source)
    subprocess.run(
        [
            "c++",
            "-std=c++20",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wno-unused-parameter",
            "-Wno-unused-variable",
            str(cpp),
            "-o",
            str(out / name),
        ],
        check=True,
    )
    result = subprocess.check_output([str(out / name)], text=True)
    check(result)
    (out / f"{name}.csv").write_text(result)
    (out / f"{name}.cpp.gz").write_bytes(gzip.compress(source.encode(), mtime=0))
for invalid in ("", expected.replace("0,64,0,0", "0,64,0,1", 1)):
    try:
        check(invalid)
    except ValueError:
        pass
    else:
        raise AssertionError("checker accepted invalid amplitude data")
report = {
    "observations_per_reference": 2048,
    "mismatches": 0,
    "negative_controls": ["empty", "one corrupted amplitude"],
    "source_manifest": json.loads((out / "base/source-hashes.json").read_text()),
    "scaling_macro_sha256": hashlib.sha256(macro.encode()).hexdigest(),
    "artifacts": {
        p.name: hashlib.sha256(p.read_bytes()).hexdigest()
        for p in out.iterdir()
        if p.suffix in (".csv", ".gz")
    },
}
(out / "comparison.json").write_text(json.dumps(report, indent=2) + "\n")
print("Both compiled references agree on all 2,048 amplitudes; invalid data rejected.")
