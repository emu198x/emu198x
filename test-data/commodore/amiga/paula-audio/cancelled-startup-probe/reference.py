"""Execute pinned audio transitions when a DAT word follows cancelled startup."""

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
common = r"""
std::set<unsigned> samples(unsigned p) { return {0,1,2,3,p+1,p+2,p+3,2*p+1,2*p+2,2*p+3}; }
"""
uae = (
    (out / "base/winuae.cpp")
    .read_text()
    .split("void run(unsigned nr,unsigned p,unsigned scenario")[0]
)
uae += (
    common
    + r"""
void run(unsigned nr,unsigned p,bool second,bool irq) {
    for(auto &ch:audio_channel) ch=audio_channel_data{};
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=0; dmacon=DMA_MASTER|(1<<nr); adkcon=0;
    auto &ch=audio_channel[nr]; ch.per=p; ch.len=ch.wlen=64;
    service(nr,false);
    if(second) dat_write(nr,0xdead);
    auto points=samples(p);
    for(now=0;now<=2*p+3;++now) {
        if(now) {
            deliver_irqs();
            if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
            if(ch.evtime==0) service(nr,true);
            if(now==1) { dmacon=0; service(nr,false); intreq=irq ? 0x80<<nr : 0; }
            if(now==2) dat_write(nr,0x1122);
        }
        if(points.count(now)) std::cout<<nr<<','<<p<<','<<second<<','<<irq<<','<<now<<','
            <<(ch.state&15)<<','<<((intreq>>(7+nr))&1)<<','<<int(ch.data.current_sample)<<','<<ch.dat<<','<<ch.wlen<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124,65536}) for(bool second:{false,true}) for(bool irq:{false,true})
        for(unsigned nr=0;nr<4;++nr) run(nr,p,second,irq);
}
"""
)
vamiga = (
    (out / "base/vamiga.cpp")
    .read_text()
    .split("template<isize nr> void run(unsigned period,unsigned scenario")[0]
)
vamiga += (
    common
    + r"""
template<isize nr> void run(unsigned period,bool second,bool irq) {
    Paula p; auto &ch=channel<nr>(p); ch.audperLatch=period; ch.audlen=ch.audlenLatch=64;
    ch.dma=true; ch.enableDMA();
    if(second) ch.pokeAUDxDAT(0xdead);
    auto points=samples(period);
    for(unsigned now=0;now<=2*period+3;++now) {
        if(now) {
            p.clock=ch.agnus.clock=now*8; p.deliver_irqs();
            if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
            if(now==1) { ch.dma=false; ch.disableDMA(); p.intreq=irq ? 0x80<<nr : 0; }
            if(now==2) ch.pokeAUDxDAT(0x1122);
        }
        if(points.count(now)) std::cout<<nr<<','<<period<<','<<second<<','<<irq<<','<<now<<','
            <<ch.state<<','<<((p.intreq>>(7+nr))&1)<<','<<ch.audioPort.sampler[nr].last/64<<','<<ch.auddat<<','<<ch.audlen<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124,65536}) for(bool second:{false,true}) for(bool irq:{false,true}) {
        run<0>(p,second,irq); run<1>(p,second,irq); run<2>(p,second,irq); run<3>(p,second,irq);
    }
}
"""
)
expected = {
    (nr, p, second, irq, t)
    for p in [1, 2, 8, 124, 65536]
    for second in range(2)
    for irq in range(2)
    for nr in range(4)
    for t in {0, 1, 2, 3, p + 1, p + 2, p + 3, 2 * p + 1, 2 * p + 2, 2 * p + 3}
}
observed = {}
for name, source in [("winuae", uae), ("vamiga", vamiga)]:
    cpp = out / f"{name}.cpp"
    cpp.write_text(source)
    (out / f"{name}.cpp.gz").write_bytes(gzip.compress(source.encode(), mtime=0))
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
    rows = [list(map(int, line.split(","))) for line in result.splitlines()]
    if (
        not rows
        or any(len(r) != 10 for r in rows)
        or len(rows) != len(expected)
        or {tuple(r[:5]) for r in rows} != expected
    ):
        raise ValueError(f"{name}: invalid inventory")
    observed[name] = rows
    (out / f"{name}.csv").write_text(result)
    print(f"{name}: {len(rows)} rows, {len({tuple(r[:4]) for r in rows})} scenarios")
differences = 0
for a, b in zip(observed["winuae"], observed["vamiga"], strict=True):
    if a[:5] != b[:5]:
        raise ValueError("input/clock disagreement")
    differences += a[5:] != b[5:]
metadata = json.loads((out / "base/source-hashes.json").read_text())
metadata["board_source_hashes"] = {
    str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
    for root, paths in [
        (args.winuae, ["custom.cpp"]),
        (
            args.vamiga,
            [
                "Core/Components/Agnus/AgnusRegs.cpp",
                "Core/Components/Agnus/AgnusEvents.cpp",
                "Core/Components/Agnus/AgnusDma.cpp",
            ],
        ),
    ]
    for path in [root / name for name in paths]
}
(out / "source-hashes.json").write_text(json.dumps(metadata, indent=2) + "\n")
(out / "comparison.json").write_text(
    json.dumps({"rows": len(expected), "differences": differences}, indent=2) + "\n"
)
print(f"Reference differences: {differences}")
