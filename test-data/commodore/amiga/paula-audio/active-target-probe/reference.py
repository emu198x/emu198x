"""Execute pinned Paula transitions with every modulation target playing."""

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
std::set<unsigned> samples(unsigned p,unsigned q) {
    std::set<unsigned> result={0,1};
    for(unsigned period:{p,q}) for(unsigned n=1;n<=4;++n)
        for(int delta:{-1,0,1}) result.insert(n*period+delta);
    return result;
}
unsigned attachment(unsigned src,unsigned mode,bool chain) {
    unsigned bits=0;
    for(unsigned nr=src;nr<3;++nr) { bits|=mode<<nr; if(!chain) break; }
    return bits;
}
unsigned initial_word(unsigned nr) { return 0x2030+nr*0x111; }
"""
uae = (
    (out / "base/winuae.cpp")
    .read_text()
    .split("void run(unsigned nr,unsigned p,unsigned scenario")[0]
)
uae += (
    common
    + r"""
void run(unsigned src,unsigned p,unsigned q,unsigned mode,bool chain,bool dma,unsigned word) {
    for(auto &ch:audio_channel) ch=audio_channel_data{};
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=0; adkcon=0; dmacon=dma ? DMA_MASTER|15 : 0;
    for(unsigned nr=0;nr<4;++nr) {
        auto &ch=audio_channel[nr]; ch.per=nr==src?p:q; ch.len=ch.wlen=64;
        if(dma) { service(nr,false); ch.dr=false; dat_write(nr,0xdead); }
        ch.dr=false; dat_write(nr,initial_word(nr));
    }
    adkcon=attachment(src,mode,chain);
    for(unsigned nr=src;nr<3;++nr) {
        audio_channel[nr].dr=false; dat_write(nr,word); if(!chain) break;
    }
    auto points=samples(p,q);
    for(now=0;now<=4*std::max(p,q)+1;++now) {
        unsigned pulse=0;
        if(now) {
            deliver_irqs(); pulse=(intreq>>7)&15; intreq=0;
            for(auto &ch:audio_channel) if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
            // WinUAE update_audio services due channels in ascending order.
            for(unsigned nr=0;nr<4;++nr) if(audio_channel[nr].evtime==0) service(nr,true);
        }
        if(points.count(now)) for(unsigned nr=0;nr<4;++nr) {
            auto &ch=audio_channel[nr];
            unsigned deadline=ch.evtime+((ch.state&0x10)?1:0);
            std::cout<<src<<','<<p<<','<<q<<','<<mode<<','<<chain<<','<<dma<<','<<word<<','<<now<<','<<nr<<','
                <<(ch.state&15)<<','<<ch.per<<','<<deadline<<','<<ch.dat2<<','<<int(ch.data.current_sample)<<','
                <<ch.data.audvol<<','<<ch.dr<<','<<((pulse>>nr)&1)<<'\n';
        }
    }
}
int main() {
    for(unsigned src=0;src<3;++src) for(unsigned p:{2,8,124}) for(unsigned q:{p-1,p,p+1})
        for(unsigned mode:{0,1,16,17}) for(bool chain:{false,true}) for(bool dma:{false,true})
            for(unsigned word:{0,1,8}) run(src,p,q,mode,chain,dma,word);
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
template<isize nr> void start(Paula &p,unsigned src,unsigned period,unsigned q,bool dma) {
    auto &ch=channel<nr>(p); ch.audperLatch=nr==src?period:q; ch.audlen=ch.audlenLatch=64;
    ch.dma=dma;
    if(dma) { ch.enableDMA(); ch.audDR=false; ch.pokeAUDxDAT(0xdead); }
    ch.audDR=false; ch.pokeAUDxDAT(initial_word(nr));
}
template<isize nr> void holding(Paula &p,unsigned src,bool chain,unsigned word) {
    if(nr>=src && nr<3 && (chain || nr==src)) { auto &ch=channel<nr>(p); ch.audDR=false; ch.pokeAUDxDAT(word); }
}
template<isize nr> void tick(Paula &p,unsigned time) {
    auto &ch=channel<nr>(p); ch.agnus.clock=time*8;
    if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
}
template<isize nr> void emit(Paula &p,unsigned src,unsigned period,unsigned q,unsigned mode,bool chain,bool dma,unsigned word,unsigned time,unsigned pulse) {
    auto &ch=channel<nr>(p);
    std::cout<<src<<','<<period<<','<<q<<','<<mode<<','<<chain<<','<<dma<<','<<word<<','<<time<<','<<nr<<','
        <<ch.state<<','<<(ch.audperLatch?ch.audperLatch:65536)<<','<<((ch.agnus.due-time*8)/8)<<','<<ch.buffer<<','
        <<ch.audioPort.sampler[nr].last/64<<','<<ch.audvolLatch<<','<<ch.audDR<<','<<((pulse>>nr)&1)<<'\n';
}
void run(unsigned src,unsigned period,unsigned q,unsigned mode,bool chain,bool dma,unsigned word) {
    Paula p;
    start<0>(p,src,period,q,dma); start<1>(p,src,period,q,dma); start<2>(p,src,period,q,dma); start<3>(p,src,period,q,dma);
    p.adkcon=attachment(src,mode,chain);
    holding<0>(p,src,chain,word); holding<1>(p,src,chain,word); holding<2>(p,src,chain,word);
    auto points=samples(period,q);
    for(unsigned time=0;time<=4*std::max(period,q)+1;++time) {
        unsigned pulse=0;
        if(time) {
            p.clock=time*8; p.deliver_irqs(); pulse=(p.intreq>>7)&15; p.intreq=0;
            // Agnus::executeUntil services SLOT_CH0, CH1, CH2, CH3 in order.
            tick<0>(p,time); tick<1>(p,time); tick<2>(p,time); tick<3>(p,time);
        }
        if(points.count(time)) {
            emit<0>(p,src,period,q,mode,chain,dma,word,time,pulse); emit<1>(p,src,period,q,mode,chain,dma,word,time,pulse);
            emit<2>(p,src,period,q,mode,chain,dma,word,time,pulse); emit<3>(p,src,period,q,mode,chain,dma,word,time,pulse);
        }
    }
}
int main() {
    for(unsigned src=0;src<3;++src) for(unsigned p:{2,8,124}) for(unsigned q:{p-1,p,p+1})
        for(unsigned mode:{0,1,16,17}) for(bool chain:{false,true}) for(bool dma:{false,true})
            for(unsigned word:{0,1,8}) run(src,p,q,mode,chain,dma,word);
}
"""
)
expected = {
    (src, p, q, mode, chain, dma, word, time, nr)
    for src in range(3)
    for p in [2, 8, 124]
    for q in [p - 1, p, p + 1]
    for mode in [0, 1, 16, 17]
    for chain in range(2)
    for dma in range(2)
    for word in [0, 1, 8]
    for time in {0, 1}
    | {n * period + d for period in [p, q] for n in range(1, 5) for d in [-1, 0, 1]}
    for nr in range(4)
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
        or {tuple(r[:9]) for r in rows} != expected
    ):
        raise ValueError(f"{name}: invalid inventory")
    observed[name] = rows
    (out / f"{name}.csv").write_text(result)
    (out / f"{name}.csv.gz").write_bytes(gzip.compress(result.encode(), mtime=0))
    print(
        f"{name}: {len(rows)} observations, {len({tuple(r[:7]) for r in rows})} scenarios"
    )
differences = [0] * 8
for a, b in zip(observed["winuae"], observed["vamiga"], strict=True):
    if a[:9] != b[:9]:
        raise ValueError("input/clock disagreement")
    for i in range(8):
        differences[i] += a[9 + i] != b[9 + i]
metadata = json.loads((out / "base/source-hashes.json").read_text())
for name, path in [
    ("winuae_scheduler", args.winuae / "audio.cpp"),
    ("vamiga_scheduler", args.vamiga / "Core/Components/Agnus/Agnus.cpp"),
]:
    metadata[name + "_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
(out / "source-hashes.json").write_text(json.dumps(metadata, indent=2) + "\n")
(out / "comparison.json").write_text(
    json.dumps(
        {
            "rows": len(expected),
            "differences": differences,
            "observables": [
                "state",
                "period",
                "counter",
                "buffer",
                "sample",
                "volume",
                "request",
                "irq",
            ],
        },
        indent=2,
    )
    + "\n"
)
print(differences)
