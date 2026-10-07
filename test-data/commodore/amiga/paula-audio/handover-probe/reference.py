"""Execute pinned WinUAE/vAmiga playback around DMA enable/disable edges."""

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
        str(Path(__file__).resolve().parent.parent / "manual-probe/reference.py"),
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
#include <set>
std::set<unsigned> edges(unsigned p) { return {1,p>1?p-1:1,p,p+1,2*p-1,2*p,2*p+1}; }
std::set<unsigned> points(unsigned p,unsigned edge) { return {0,1,edge-1,edge,edge+1,edge+2,p,2*p-1,2*p,2*p+1,3*p,3*p+1}; }
"""
uae = (out / "base/winuae.cpp").read_text().split("#include <set>")[0]
uae = uae.replace(
    "void setdr(int,bool) { std::abort(); }",
    "void setdr(int nr,bool restart) { audio_channel[nr].dr=true; audio_channel[nr].dsr|=restart; }",
)
uae = uae.replace(
    "// Manual-only probe must never request DMA.",
    "// Record requests; no words are granted after startup in this adapter.",
)
# The only unconditional log is a disabled compatibility-hack diagnostic.
uae = uae.replace(
    "void write_log(const char*,...) { std::abort(); }",
    "void write_log(const char*,...) {}",
)
# Disable the host performance workaround, as with usehacks/PWM above.
uae = uae.replace(
    "void audio_low_period_hack(audio_channel_data*) { std::abort(); }",
    "void audio_low_period_hack(audio_channel_data*) {}",
)
uae += (
    common
    + r"""
void service(int nr,bool perfin) { if(audio_state_channel2(nr,perfin)) audio_channel[nr].dat_written=false; }
void audio_state_channel(int nr,bool perfin) { assert(nr>=0 && nr<4); service(nr,perfin); }
void dat_write(unsigned nr,unsigned word) {
    bool enabled=(dmacon&DMA_MASTER) && (dmacon&(1<<nr));
    if(enabled) { audio_channel[nr].dat=word; audio_channel[nr].dat_written=true; }
    event_audxdat_func(nr | (enabled?0x80:0) | (word<<8));
}
void run(unsigned nr,unsigned p,unsigned scenario,unsigned edge,bool after) {
    for(auto &ch:audio_channel) ch=audio_channel_data{};
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=0; dmacon=0; adkcon=0;
    auto &ch=audio_channel[nr]; ch.per=p; ch.len=ch.wlen=scenario==7 ? 1 : 64;
    auto dma=[&](bool enabled) { dmacon=enabled ? DMA_MASTER|(1<<nr) : 0; service(nr,false); };
    if(scenario!=0 && scenario!=6) { dma(true); dat_write(nr,0xdead); }
    dat_write(nr,0x1122);
    if(scenario==4 || scenario==7) dat_write(nr,0x3344);
    auto observations=points(p,edge);
    for(now=0;now<=3*p+1;++now) {
        auto apply=[&] {
            if(now==edge) dma(scenario==0 || scenario==6);
            if((scenario==3 || scenario==7) && now==edge+1) dma(true);
            if(scenario==6 && now==edge+1) dma(false);
        };
        if(now) {
            deliver_irqs();
            if((scenario==7 && now==1) || scenario==2 || scenario==4 || ((scenario==5 || scenario==6) && now==2*p)) intreq&=~(0x80<<nr);
            if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
            if(!after) apply();
            if(ch.evtime==0) service(nr,true);
            if(after) apply();
        }
        if(observations.count(now)) std::cout<<nr<<','<<p<<','<<scenario<<','<<edge<<','<<after<<','<<now<<','
            <<(ch.state&15)<<','<<((intreq>>(7+nr))&1)<<','<<int(ch.data.current_sample)<<','<<ch.intreq2<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124}) for(unsigned s=0;s<8;++s) for(unsigned edge:edges(p)) for(bool after:{false,true})
        for(unsigned nr=0;nr<4;++nr) run(nr,p,s,edge,after);
}
"""
)
vamiga = (out / "base/vamiga.cpp").read_text().split("#include <set>")[0]
vamiga = vamiga.replace(
    "void serviceEvent();",
    "void serviceEvent(); void enableDMA(); void disableDMA(); void move_001_000(); void move_101_000(); void move_010_000();",
)
state = args.vamiga / "Core/Components/Paula/Audio/StateMachine.cpp"
raw = state.read_text()
for name in ["enableDMA", "disableDMA", "move_001_000", "move_101_000", "move_010_000"]:
    start = raw.index(f"template <isize nr> void\nStateMachine<nr>::{name}(")
    brace = raw.index("{", start)
    depth = 1
    end = brace + 1
    while depth:
        depth += (raw[end] == "{") - (raw[end] == "}")
        end += 1
    vamiga += raw[start:end] + "\n"
vamiga += (
    common
    + r"""
template<isize nr> StateMachine<nr>& channel(Paula &p) {
    if constexpr(nr==0) return p.channel0;
    if constexpr(nr==1) return p.channel1;
    if constexpr(nr==2) return p.channel2;
    if constexpr(nr==3) return p.channel3;
}
template<isize nr> void run(unsigned period,unsigned scenario,unsigned edge,bool after) {
    Paula p; auto &ch=channel<nr>(p); ch.audperLatch=period; ch.audlen=ch.audlenLatch=scenario==7 ? 1 : 64;
    auto dma=[&](bool enabled) { ch.dma=enabled; if(enabled) ch.enableDMA(); else ch.disableDMA(); };
    if(scenario!=0 && scenario!=6) { dma(true); ch.pokeAUDxDAT(0xdead); }
    ch.pokeAUDxDAT(0x1122);
    if(scenario==4 || scenario==7) ch.pokeAUDxDAT(0x3344);
    auto observations=points(period,edge);
    for(unsigned now=0;now<=3*period+1;++now) {
        auto apply=[&] { if(now==edge) dma(scenario==0 || scenario==6); if((scenario==3 || scenario==7) && now==edge+1) dma(true);
            if(scenario==6 && now==edge+1) dma(false); };
        if(now) {
            p.clock=ch.agnus.clock=now*8; p.deliver_irqs();
            if((scenario==7 && now==1) || scenario==2 || scenario==4 || ((scenario==5 || scenario==6) && now==2*period)) p.intreq&=~(0x80<<nr);
            if(!after) apply();
            if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
            if(after) apply();
        }
        if(observations.count(now)) std::cout<<nr<<','<<period<<','<<scenario<<','<<edge<<','<<after<<','<<now<<','
            <<ch.state<<','<<((p.intreq>>(7+nr))&1)<<','<<ch.audioPort.sampler[nr].last/64<<','<<ch.intreq2<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124}) for(unsigned s=0;s<8;++s) for(unsigned edge:edges(p)) for(bool after:{false,true}) {
        run<0>(p,s,edge,after); run<1>(p,s,edge,after); run<2>(p,s,edge,after); run<3>(p,s,edge,after);
    }
}
"""
)
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
    expected = {
        (nr, p, s, edge, int(after), t)
        for p in [1, 2, 8, 124]
        for s in range(8)
        for edge in {1, max(1, p - 1), p, p + 1, 2 * p - 1, 2 * p, 2 * p + 1}
        for after in [False, True]
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
        }
        if t <= 3 * p + 1
    }
    if (
        not rows
        or any(len(r) != 10 for r in rows)
        or len(rows) != len(expected)
        or {tuple(r[:6]) for r in rows} != expected
    ):
        raise ValueError(f"{name}: invalid inventory")
    observed[name] = rows
    (out / f"{name}.csv").write_text(result)
    print(f"{name}: {len(rows)} rows, {len({tuple(r[:5]) for r in rows})} scenarios")
differences = [0] * 8
for a, b in zip(observed["winuae"], observed["vamiga"], strict=True):
    if a[:6] != b[:6]:
        raise ValueError("input/clock disagreement")
    differences[a[2]] += a[6:8] != b[6:8]
metadata = json.loads((out / "base/source-hashes.json").read_text())
metadata["handover_vamiga_state_sha256"] = hashlib.sha256(raw.encode()).hexdigest()
(out / "source-hashes.json").write_text(json.dumps(metadata, indent=2) + "\n")
(out / "comparison.json").write_text(
    json.dumps(
        {
            "rows": len(observed["winuae"]),
            "state_irq_differences_by_scenario": differences,
            "dac_excluded": "vAmiga repeated-edge suppression",
        },
        indent=2,
    )
    + "\n"
)
print(differences)
