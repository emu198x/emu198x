"""Execute pinned Paula transitions around live attachment changes."""

import argparse
import gzip
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
std::set<unsigned> samples(unsigned p,unsigned edge) { return {0,1,edge-1,edge,edge+1,edge+2,p,2*p-1,2*p,2*p+1,3*p,3*p+1,4*p,4*p+1}; }
"""
uae = (
    (out / "base/winuae.cpp")
    .read_text()
    .split("void run(unsigned nr,unsigned p,unsigned scenario")[0]
)
uae += (
    common
    + r"""
void run(unsigned nr,unsigned p,bool dma,unsigned from,unsigned to,unsigned edge,bool attach_first) {
    for(auto &ch:audio_channel) { ch=audio_channel_data{}; ch.per=31; }
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=0; dmacon=dma ? DMA_MASTER|(1<<nr) : 0; adkcon=from<<nr;
    auto &ch=audio_channel[nr]; ch.per=p; ch.len=ch.wlen=1;
    if(dma) { service(nr,false); ch.dr=false; dat_write(nr,0xdead); }
    ch.dr=false; dat_write(nr,0x1122);
    bool delivered=false; unsigned pulse=0;
    auto points=samples(p,edge);
    for(now=0;now<=4*p+1;++now) {
        if(now) {
            deliver_irqs(); pulse=(intreq>>(7+nr))&1; intreq=0;
            if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
            if(now==edge && attach_first) adkcon=to<<nr;
            if(!delivered && now>=edge && (!dma || ch.dr)) { ch.dr=false; dat_write(nr,0x3344); delivered=true; }
            if(now==edge && !attach_first) adkcon=to<<nr;
            if(ch.evtime==0) service(nr,true);
        }
        if(points.count(now)) std::cout<<nr<<','<<p<<','<<dma<<','<<from<<','<<to<<','<<edge<<','<<attach_first<<','<<now<<','
            <<(ch.state&15)<<','<<pulse<<','<<ch.dat2<<','<<int(ch.data.current_sample)<<','
            <<(nr<3?audio_channel[nr+1].per:0)<<','<<(nr<3?audio_channel[nr+1].data.audvol:0)<<','
            <<ch.dr<<','<<ch.intreq2<<','<<delivered<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124}) for(bool dma:{false,true}) for(unsigned from:{0,1,16,17}) for(unsigned to:{0,1,16,17})
        for(unsigned edge:edges(p)) for(bool first:{false,true}) for(unsigned nr=0;nr<4;++nr) run(nr,p,dma,from,to,edge,first);
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
template<isize nr> void run(unsigned period,bool dma,unsigned from,unsigned to,unsigned edge,bool attach_first) {
    Paula p; p.channel0.audperLatch=p.channel1.audperLatch=p.channel2.audperLatch=p.channel3.audperLatch=31;
    auto &ch=channel<nr>(p); ch.audperLatch=period; ch.audlen=ch.audlenLatch=1;
    ch.dma=dma; p.adkcon=from<<nr;
    if(dma) { ch.enableDMA(); ch.audDR=false; ch.pokeAUDxDAT(0xdead); }
    ch.audDR=false; ch.pokeAUDxDAT(0x1122);
    bool delivered=false; unsigned pulse=0;
    auto points=samples(period,edge);
    for(unsigned now=0;now<=4*period+1;++now) {
        if(now) {
            p.clock=ch.agnus.clock=now*8; p.deliver_irqs(); pulse=(p.intreq>>(7+nr))&1; p.intreq=0;
            if(now==edge && attach_first) p.adkcon=to<<nr;
            if(!delivered && now>=edge && (!dma || ch.audDR)) { ch.audDR=false; ch.pokeAUDxDAT(0x3344); delivered=true; }
            if(now==edge && !attach_first) p.adkcon=to<<nr;
            if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
        }
        unsigned target_period=0,target_volume=0;
        if constexpr(nr<3) { auto &target=channel<nr+1>(p); target_period=target.audperLatch?target.audperLatch:65536; target_volume=target.audvolLatch; }
        if(points.count(now)) std::cout<<nr<<','<<period<<','<<dma<<','<<from<<','<<to<<','<<edge<<','<<attach_first<<','<<now<<','
            <<ch.state<<','<<pulse<<','<<ch.buffer<<','<<ch.audioPort.sampler[nr].last/64<<','
            <<target_period<<','<<target_volume<<','<<ch.audDR<<','<<ch.intreq2<<','<<delivered<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124}) for(bool dma:{false,true}) for(unsigned from:{0,1,16,17}) for(unsigned to:{0,1,16,17})
        for(unsigned edge:edges(p)) for(bool first:{false,true}) {
            run<0>(p,dma,from,to,edge,first); run<1>(p,dma,from,to,edge,first); run<2>(p,dma,from,to,edge,first); run<3>(p,dma,from,to,edge,first);
        }
}
"""
)
expected = {
    (nr, p, dma, before, after, edge, first, t)
    for p in [1, 2, 8, 124]
    for dma in range(2)
    for before in [0, 1, 16, 17]
    for after in [0, 1, 16, 17]
    for edge in {1, max(1, p - 1), p, p + 1, 2 * p - 1, 2 * p, 2 * p + 1}
    for first in range(2)
    for nr in range(4)
    for t in {
        0,
        1,
        edge - 1,
        edge,
        edge + 1,
        edge + 2,
        p,
        2 * p - 1,
        2 * p,
        2 * p + 1,
        3 * p,
        3 * p + 1,
        4 * p,
        4 * p + 1,
    }
    if t <= 4 * p + 1
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
        or any(len(r) != 17 for r in rows)
        or len(rows) != len(expected)
        or {tuple(r[:8]) for r in rows} != expected
    ):
        raise ValueError(f"{name}: invalid inventory")
    observed[name] = rows
    (out / f"{name}.csv.gz").write_bytes(gzip.compress(result.encode(), mtime=0))
    (out / f"{name}.csv").write_text(result)
    print(f"{name}: {len(rows)} rows, {len({tuple(r[:7]) for r in rows})} scenarios")
differences = [0] * 9
for a, b in zip(observed["winuae"], observed["vamiga"], strict=True):
    if a[:8] != b[:8]:
        raise ValueError("input/clock disagreement")
    for i in range(9):
        differences[i] += a[8 + i] != b[8 + i]
(out / "comparison.json").write_text(
    json.dumps(
        {
            "rows": len(expected),
            "differences_by_observable": differences,
            "observables": [
                "state",
                "irq",
                "buffer",
                "sample",
                "target_period",
                "target_volume",
                "request",
                "loop",
                "delivered",
            ],
        },
        indent=2,
    )
    + "\n"
)
(out / "source-hashes.json").write_text((out / "base/source-hashes.json").read_text())
print(differences)
