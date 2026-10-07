"""Compile unmodified registered DDF functions with explicit external inputs."""

import argparse
import csv
import hashlib
import io
import json
import subprocess
from pathlib import Path

SOURCE_SHA256 = "75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5"


def extract(source: str, start: str, end: str) -> str:
    begin = source.index(start)
    return source[begin : source.index(end, begin)]


def build(source_path: Path, output: Path, reverse_order: bool) -> None:
    original = source_path.read_bytes()
    if hashlib.sha256(original).hexdigest() != SOURCE_SHA256:
        raise SystemExit("registered custom.cpp source hash differs")
    source = original.decode()
    fragments = {
        "cadence_tables": extract(
            source,
            "static const uae_u8 fetchunits[]",
            "static uae_s8 cycle_diagram_table",
        ),
        "plane_tables": extract(
            source, "static const uae_u8 bpl_sequence_8", "/* set currently active"
        ),
        "start": extract(
            source, "static void bprun_start(int hpos)", "/*\n\tCPU write"
        ),
        "generate": extract(
            source,
            "static void bpl_dma_normal_stop(int hpos)",
            "static void update_bpl_scandoubler",
        ),
        "decide": extract(
            source, "static void decide_bpl(int hpos)", "static void check_bpl_vdiw"
        ),
        "clock": extract(
            source, "static void get_cck_clock(void)", "static void inc_cck(void)"
        ),
    }
    # Establish the actual ordering from the enclosing registered functions.
    requests = extract(
        source, "static void generate_dma_requests(void)", "static void do_cck("
    )
    cck = extract(source, "static void do_cck(bool docycles)", "\n\tcheck_hsyncs();")
    if "generate_bpl(cck_clock);" not in requests or not (
        cck.index("generate_dma_requests();") < cck.index("decide_bpl(agnus_hpos);")
    ):
        raise SystemExit("registered request/comparator ordering differs")
    output.mkdir(parents=True, exist_ok=True)
    prelude = r"""#include <cstdio>
#include <cstdlib>
#include <cstdint>
using uae_u8=uint8_t;
#define STATIC_INLINE static inline
struct rgabuf { int bpldat; };
rgabuf request;
constexpr int RGA_SLOT_BPL=1, CYCLE_BITPLANE=1, MAX_SCANDOUBLED_LINES=625;
int agnus_hpos, maxhpos, maxhpos_long, agnus_pos_change, agnus_hpos_next=-1;
bool cck_clock;
int bprun,bprun_cycle,ddf_stopping,ddf_enable_on,ddfstrt,ddfstop;
bool ddf_limit,ddfstrt_match,hwi_old,ecs_agnus,aga_mode,harddis_h,dmacon_bpl;
int fetchunit_mask,fetchstart_mask,fm_maxplane,bplcon0_planes,bplcon0_planes_limit;
int plfstrt_sprite,ddflastword_total,ddffirstword_total,linear_vpos,bprun_end;
enum class diw_states { DIW_waiting_start, DIW_waiting_stop };
diw_states vdiwstate;
struct { bool gfx_scandoubler; } currprefs;
int requested_plane=-1;
void bpl_autoscale() {} // Autoscale metadata is outside the sequencer.
void update_bpl_scandoubler() {} // Disabled by the harness's preferences.
int get_cycles() { return linear_vpos*maxhpos+agnus_hpos; }
rgabuf *write_rga(int slot,int type,int reg,void *pointer) {
 if(slot!=RGA_SLOT_BPL || type!=CYCLE_BITPLANE || pointer ||
    requested_plane!=-1 || reg<0x110 || reg>=0x120 || (reg&1)) std::exit(3);
 requested_plane=(reg-0x110)/2;
 request={}; return &request;
}
"""
    main = r"""int main(int argc,char **argv) {
 bool reverse=argc==2 && argv[1][0]=='r';
 int id=0;
 std::puts("case,line,h,clock,plane,mod,run_before,cycle_before,stop_before,run_after,cycle_after,stop_after,soft_after,limit_after,hwi_after");
 for(int chip=0;chip<3;chip++)
 for(int mode=0;mode<(chip==2?4:1);mode++)
 for(int res=0;res<(chip==0?2:3);res++)
 for(int scenario=0;scenario<10;scenario++)
 for(int length=227;length<=228;length++) {
  if(scenario==9 && chip==0) continue;
  ecs_agnus=chip!=0; aga_mode=chip==2; harddis_h=scenario==9 || (chip!=0 && res==2);
  maxhpos=maxhpos_long=length;
  int fm=mode==0?0:mode==3?2:1;
  fetchunit_mask=fetchunits[fm*4+res]-1;
  fetchstart_mask=(1<<fetchstarts[fm*4+res])-1;
  fm_maxplane=1<<fm_maxplanes[fm*4+res];
  bpl_sequence=fm_maxplane==8?bpl_sequence_8:fm_maxplane==4?bpl_sequence_4:bpl_sequence_2;
  bplcon0_planes=bplcon0_planes_limit=(chip!=2 && fm_maxplane>6)?6:fm_maxplane;
  bprun=bprun_cycle=ddf_stopping=ddf_enable_on=0;
  ddf_limit=ddfstrt_match=hwi_old=false;
  plfstrt_sprite=0x100; ddflastword_total=0; ddffirstword_total=0x100;
  dmacon_bpl=true; vdiwstate=diw_states::DIW_waiting_stop;
  ddfstrt=scenario==1?60:scenario==3?28:scenario==4?16:56;
  ddfstop=scenario==2?56:scenario==3?232:scenario==7?96:208;
  std::fprintf(stderr,"%d,%d,%d,%d,%d,%d\n",id,chip,mode,res,length,scenario);
  for(linear_vpos=0;linear_vpos<2;linear_vpos++)
  for(agnus_hpos=0;agnus_hpos<length;agnus_hpos++) {
   // External signals are supplied before this CCK; no DMACON-write latency is claimed.
   if(scenario==5) {
    if(agnus_hpos==80) dmacon_bpl=false;
    if(agnus_hpos==88) ddfstrt=112;
    if(agnus_hpos==96) dmacon_bpl=true;
   }
   if(scenario==6) {
    if(agnus_hpos==214) dmacon_bpl=false;
    if(agnus_hpos==222) dmacon_bpl=true;
   }
   if(scenario==7 && agnus_hpos==112) { ddfstrt=128; ddfstop=176; }
   if(scenario==8) {
    if(agnus_hpos==80) vdiwstate=diw_states::DIW_waiting_start;
    if(agnus_hpos==96) vdiwstate=diw_states::DIW_waiting_stop;
   }
   requested_plane=-1; request={};
   int rb=bprun,cb=bprun_cycle,sb=ddf_stopping,eb=ddf_enable_on;
   bool lb=ddf_limit,hb=hwi_old;
   get_cck_clock();
   if(reverse) { decide_bpl(agnus_hpos); generate_bpl(cck_clock); }
   else { generate_bpl(cck_clock); decide_bpl(agnus_hpos); }
   if(requested_plane>=0 || rb!=bprun || sb!=ddf_stopping || eb!=ddf_enable_on ||
      lb!=ddf_limit || hb!=hwi_old) {
    std::printf("%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d,%d\n",
     id,linear_vpos,agnus_hpos,int(cck_clock),requested_plane,
     requested_plane>=0 && (request.bpldat&8)?1:0,
     rb,cb,sb,bprun,bprun_cycle,ddf_stopping,ddf_enable_on,int(ddf_limit),int(hwi_old));
   }
  }
  id++;
 }
 return id==336?0:4;
}
"""
    cpp = output / "reference-sequencer.cpp"
    cpp.write_text(prelude + "\n".join(fragments.values()) + main)
    binary = output / "reference-sequencer"
    subprocess.run(
        ["clang++", "-std=c++17", "-O2", str(cpp), "-o", str(binary)], check=True
    )
    result = subprocess.run(
        [str(binary)] + (["reverse"] if reverse_order else []),
        capture_output=True,
        text=True,
        check=True,
    )
    cases_header = "case,chip,mode,res,line_ccks,scenario\n"
    cases = list(csv.DictReader(io.StringIO(cases_header + result.stderr)))
    events = list(csv.DictReader(io.StringIO(result.stdout)))
    if len(cases) != 336 or len({row["case"] for row in cases}) != 336:
        raise SystemExit("case coverage differs")
    requests_rows = [row for row in events if int(row["plane"]) >= 0]
    if not requests_rows or {row["case"] for row in requests_rows} != {
        row["case"] for row in cases
    }:
        raise SystemExit("missing request coverage")
    first_bpl1 = next(
        int(row["h"])
        for row in requests_rows
        if row["case"] == "0" and row["line"] == "0" and row["plane"] == "0"
    )
    if first_bpl1 != 65:
        raise SystemExit(
            f"request-before-comparator gate failed: first BPL1 h={first_bpl1}, expected 65"
        )
    if source_path.read_bytes() != original:
        raise SystemExit("reference source changed during compilation")
    (output / "registered-cases.csv").write_text(cases_header + result.stderr)
    (output / "registered-events.csv").write_text(result.stdout)
    report = {
        "source_sha256": SOURCE_SHA256,
        "fragment_sha256": {
            name: hashlib.sha256(fragment.encode()).hexdigest()
            for name, fragment in fragments.items()
        },
        "cases": len(cases),
        "events": len(events),
        "requests": len(requests_rows),
        "terminal_mod_requests": sum(int(row["mod"]) for row in requests_rows),
        "first_normal_bpl1_request_h": first_bpl1,
        "events_sha256": hashlib.sha256(result.stdout.encode()).hexdigest(),
        "cases_sha256": hashlib.sha256(
            (cases_header + result.stderr).encode()
        ).hexdigest(),
        "scope": "Compiled registered DDF sequencer functions with external inputs; no full bus arbitration, memory service, CPU, RGA delay or raster simulation",
    }
    (output / "verification.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="registered FS-UAE custom.cpp")
    parser.add_argument("output", type=Path, help="scratch directory")
    parser.add_argument(
        "--reverse-order", action="store_true", help="negative control; must fail"
    )
    arguments = parser.parse_args()
    build(arguments.source, arguments.output, arguments.reverse_order)
