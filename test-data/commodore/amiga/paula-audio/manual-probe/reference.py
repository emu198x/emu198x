"""Execute unmodified WinUAE and vAmiga manual audio transitions at IRQ boundaries."""

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
revisions = {
    "winuae": "c32694e338fa5f34977f522eb4898adb069d2e73",
    "vamiga": "60fd1e6b69dcd77c9f44d1291bd37ec715362ab0",
}
for name, root in [("winuae", args.winuae), ("vamiga", args.vamiga)]:
    actual = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=root, text=True
    ).strip()
    if actual != revisions[name]:
        raise ValueError(f"unregistered {name} revision: {actual}")


def extract(source: str, signature: str) -> str:
    start = source.index(signature)
    brace = source.index("{", start)
    depth, end = 1, brace + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return source[start:end] + "\n"


common = r"""
#include <set>
// Events before/after period service distinguish simultaneous-edge ordering.
bool action(unsigned scenario, unsigned t, unsigned period) {
    switch(scenario) {
    case 0: return false; // no acknowledgement
    case 1: return t==1; // early acknowledgement
    case 2: return t==2*period-2;
    case 3: return t==2*period-1;
    case 4: return t==2*period;
    case 5: return t==2*period+1;
    case 6: return t==1 || t==2*period; // acknowledge early; set again at edge
    case 7: return t==1 || t==2*period-1; // set at sample edge
    case 8: return t==1 || t==period/2; // holding write in high phase
    case 9: return t==1 || t==period+period/2; // holding write in low phase
    case 10: return false; // pending IRQ blocks startup
    }
    std::abort();
}
std::set<unsigned> observations(unsigned p) {
    return {0,1,p-1,p,p+1,2*p-2,2*p-1,2*p,2*p+1,2*p+2,3*p,3*p+1};
}
"""
uae_prefix = r"""
// Extracted WinUAE audio methods; upstream copyright and terms apply.
// Separate diagnostic, never linked into Emu198x.
#include <algorithm>
#include <cassert>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iostream>
using uae_u16=uint16_t; using uae_u32=uint32_t; using uaecptr=uint32_t;
using sample8_t=int8_t;
constexpr int CYCLE_UNIT=1, DMA_MASTER=0x200, PERIOD_MIN=1;
constexpr unsigned MAX_EV=0xffffffffU;
#define DEBUG_AUDIO 0
#define DEBUG_AUDIO2 0
#define DEBUG_AUDIO_HACK 0
#define TEST_AUDIO 0
#define TEST_MANUAL_AUDIO 0
#define TEST_MISSED_DMA 0
#define M68K_GETPC 0
#define _T(x) x
struct { int produce_sound=1,cachesize=0; bool sound_volcnt=false,cpu_memory_cycle_exact=true; } currprefs;
struct { unsigned instruction_cnt=0; } regs;
struct audio_channel_data {
    unsigned evtime=MAX_EV;
    bool dmaenstore=false,intreq2=false,dr=false,dsr=false,pbufldl=false,dat_written=false;
    int irqcheck=0,state=0,per=8,len=1,wlen=1,volcnt=0,volcntbufcnt=0,minperloop=0;
    uint16_t dat=0,dat2=0;
    unsigned lc=0,pt=0,ptx=0,dmaofftime_cpu_cnt=0,dmaofftime_pc=0;
    bool ptx_written=false,ptx_tofetch=false,dmaofftime_active=false;
    float volcntbuf[1]{};
    struct { int audvol=64,mixvol=64; int8_t current_sample=0,last_sample=0,new_sample=0; } data;
} audio_channel[4];
unsigned dmacon=0,adkcon=0,now=0,irq_due[16],intreq=0;
int audio_channel_mask=15,sampleripper_enabled=0;
unsigned INTREQR() { return intreq; }
void INTREQ_INT(unsigned n,unsigned delay) { irq_due[n]=std::min(irq_due[n],now+delay); }
void deliver_irqs() { for(unsigned n=0;n<16;++n) if(now>=irq_due[n]) { intreq|=1<<n; irq_due[n]=MAX_EV; } }
bool usehacks() { return false; }
int current_hpos() { return 0; }
void audio_activate() {}
void write_log(const char*,...) { std::abort(); }
void setdr(int,bool) { std::abort(); } // Manual-only probe must never request DMA.
void do_samplerip(audio_channel_data*) { std::abort(); }
void audio_low_period_hack(audio_channel_data*) { std::abort(); }
void audio_state_channel(int,bool);
void update_audio() {} // External adapter has already advanced to the write clock.
void schedule_audio() {} // The adapter reads evtime after every operation.
void events_schedule() {}
"""
uae_source = (args.winuae / "audio.cpp").read_text()
uae = uae_source[: uae_source.index("#include")] + uae_prefix
for signature in [
    "static void zerostate(int nr, bool reset)",
    "static void update_volume(int nr, uae_u16 v)",
    "static int isirq(int nr)",
    "static void setirq(int nr, int which)",
    "static void newsample(int nr, sample8_t sample)",
    "static void loaddat (int nr, bool modper)",
    "static void loaddat (int nr)",
    "static void loadper1(int nr)",
    "static void loadperm1(int nr)",
    "static void loadper (int nr)",
    "static bool audio_state_channel2 (int nr, bool perfin)",
    "void event_audxdat_func(uae_u32 v)",
]:
    uae += extract(uae_source, signature)
uae += (
    common
    + r"""
void service(int nr,bool perfin) {
    if(audio_state_channel2(nr,perfin)) audio_channel[nr].dat_written=false;
}
void audio_state_channel(int nr,bool perfin) { assert(nr>=0 && nr<4); service(nr,perfin); }
void dat_write(int nr,unsigned word) { event_audxdat_func(nr | (word<<8)); }
void run(unsigned nr,unsigned period,unsigned scenario,bool after) {
    for(auto &ch:audio_channel) ch=audio_channel_data{};
    for(auto &due:irq_due) due=MAX_EV;
    now=0; intreq=scenario==10 ? 0x80<<nr : 0;
    auto &ch=audio_channel[nr]; ch.per=period;
    dat_write(nr,0x1122);
    auto points=observations(period);
    auto apply=[&] {
        if(!action(scenario,now,period)) return;
        if((scenario==6 && now==2*period)||(scenario==7 && now==2*period-1)) intreq|=0x80<<nr;
        else if((scenario==8 && now==period/2)||(scenario==9 && now==period+period/2)) dat_write(nr,0x3344);
        else intreq&=~(0x80<<nr);
    };
    for(now=0;now<=3*period+1;++now) {
        if(now) {
            deliver_irqs();
            if(ch.evtime!=MAX_EV) { assert(ch.evtime>0); --ch.evtime; }
            if(!after) apply();
            if(ch.evtime==0) service(nr,true);
            if(after) apply();
        }
        if(points.count(now)) std::cout<<nr<<','<<period<<','<<scenario<<','<<after<<','<<now<<','
            <<(ch.state&15)<<','<<((intreq>>(7+nr))&1)<<','<<int(ch.data.current_sample)<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124,65536}) for(unsigned s=0;s<11;++s) for(bool after:{false,true})
        for(unsigned nr=0;nr<4;++nr) run(nr,p,s,after);
}
"""
)
# Regenerate the established vAmiga extraction from the pinned source first.
subprocess.run(
    [
        "python3.13",
        str(Path(__file__).resolve().parent.parent / "interrupt-probe/reference.py"),
        "--source",
        str(args.vamiga.resolve()),
        "--output",
        str(out / "vamiga-base"),
    ],
    check=True,
)
vamiga = (
    (out / "vamiga-base/reference.cpp")
    .read_text()
    .split("template<isize nr> StateMachine<nr>& channel")[0]
)
vamiga += (
    common
    + r"""
template<isize nr> StateMachine<nr>& channel(Paula &p) {
    if constexpr(nr==0) return p.channel0;
    if constexpr(nr==1) return p.channel1;
    if constexpr(nr==2) return p.channel2;
    if constexpr(nr==3) return p.channel3;
}
template<isize nr> void run(unsigned period,unsigned scenario,bool after) {
    Paula p; auto &ch=channel<nr>(p); ch.audperLatch=period;
    if(scenario==10) p.intreq=0x80<<nr;
    ch.pokeAUDxDAT(0x1122);
    auto points=observations(period);
    for(unsigned now=0;now<=3*period+1;++now) {
        auto apply=[&] {
            if(!action(scenario,now,period)) return;
            if((scenario==6 && now==2*period)||(scenario==7 && now==2*period-1)) p.intreq|=0x80<<nr;
            else if((scenario==8 && now==period/2)||(scenario==9 && now==period+period/2)) ch.pokeAUDxDAT(0x3344);
            else p.intreq&=~(0x80<<nr);
        };
        if(now) {
            p.clock=ch.agnus.clock=now*8; p.deliver_irqs();
            if(!after) apply();
            if(ch.agnus.due==ch.agnus.clock) ch.serviceEvent();
            if(after) apply();
        }
        if(points.count(now)) std::cout<<nr<<','<<period<<','<<scenario<<','<<after<<','<<now<<','
            <<ch.state<<','<<((p.intreq>>(7+nr))&1)<<','<<ch.audioPort.sampler[nr].last/64<<'\n';
    }
}
int main() {
    for(unsigned p:{1,2,8,124,65536}) for(unsigned s=0;s<11;++s) for(bool after:{false,true}) {
        run<0>(p,s,after); run<1>(p,s,after); run<2>(p,s,after); run<3>(p,s,after);
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
    rows = [list(map(int, row.split(","))) for row in result.splitlines()]
    if len(rows) != 4312 or any(len(row) != 8 for row in rows):
        raise ValueError(f"{name}: invalid observation inventory")
    cases = {tuple(row[:4]) for row in rows}
    expected_cases = {
        (nr, period, scenario, after)
        for period in [1, 2, 8, 124, 65536]
        for scenario in range(11)
        for after in [0, 1]
        for nr in range(4)
    }
    if cases != expected_cases:
        raise ValueError(f"{name}: incomplete scenario inventory")
    observed[name] = rows
    (out / f"{name}.csv").write_text(result)
    print(f"{name}: {len(result.splitlines())} observations")
differences = []
for a, b in zip(observed["winuae"], observed["vamiga"], strict=True):
    if a[:5] != b[:5]:
        raise ValueError("reference clock/input schedules disagree")
    if a[5:7] != b[5:7]:
        differences.append(a)
comparison = {
    "rows_per_producer": 4312,
    "scenarios_per_producer": 440,
    "state_irq_differences": len(differences),
    "differing_scenarios": len({tuple(row[:4]) for row in differences}),
    "dac_excluded": "vAmiga repeated-edge suppression",
}
(out / "comparison.json").write_text(json.dumps(comparison, indent=2) + "\n")
print(comparison)
metadata = {
    "revisions": revisions,
    "winuae_audio_sha256": hashlib.sha256(uae_source.encode()).hexdigest(),
    "winuae_changelog_sha256": hashlib.sha256(
        (args.winuae / "od-win32/winuaechangelog.txt").read_bytes()
    ).hexdigest(),
    "vamiga_sources": json.loads((out / "vamiga-base/source-hashes.json").read_text()),
}
(out / "source-hashes.json").write_text(json.dumps(metadata, indent=2) + "\n")
