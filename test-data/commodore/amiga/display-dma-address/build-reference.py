import argparse
import hashlib
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Compile registered FS-UAE PT/MOD sampling without editing its source"
)
parser.add_argument("source", type=Path, help="registered custom.cpp")
parser.add_argument("output", type=Path, help="scratch output directory")
args = parser.parse_args()
base = args.output
base.mkdir(parents=True, exist_ok=True)
source = args.source
expected = "75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5"
if hashlib.sha256(source.read_bytes()).hexdigest() != expected:
    raise SystemExit("registered custom.cpp source hash differs")
s = source.read_text()
start = s.index("static void bitplane_rga_ptmod(void)")
end = s.index("// Generate UHRES", start)
function = s[start:end]
pre = """#include <cstdio>
#include <cstdint>
using uaecptr=uint32_t;
constexpr int CYCLE_BITPLANE=1, CYCLE_SPRITE=2;
struct rgabuf { int alloc,type,bpldat,sprdat; uaecptr *p,pv; int bplmod; };
struct sprite { uaecptr pt; };
rgabuf cell; sprite spr[8]; uaecptr bplpt[8];
int fmode,diwstrt,vpos,bpl1mod,bpl2mod;
rgabuf *read_rga_in() { return &cell; }
"""
main = """int main() {
  int rows=0;
  for (int plane=0;plane<8;plane++)
  for (int mod=0;mod<2;mod++)
  for (int line=60;line<62;line++)
  for (int start=44;start<46;start++)
  for (int select=0;select<2;select++) {
    cell={}; cell.alloc=1; cell.type=CYCLE_BITPLANE;
    cell.bpldat=plane|(mod?8:0); bplpt[plane]=0x2000+plane*0x100;
    bpl1mod=-4; bpl2mod=6; vpos=line; diwstrt=start<<8; fmode=select?0x4000:0;
    bitplane_rga_ptmod();
    std::printf("%d,%d,%d,%d,%d,%u,%d\\n",plane,mod,line,start,select,cell.pv,cell.bplmod);
    rows++;
  }
  return rows==128?0:1;
}
"""
cpp = base / "reference-address.cpp"
cpp.write_text(pre + function + main)
subprocess.run(
    ["clang++", "-std=c++17", str(cpp), "-o", str(base / "reference-address")],
    check=True,
)
result = subprocess.run(
    [str(base / "reference-address")], capture_output=True, text=True, check=True
)
rows = result.stdout.strip().splitlines()
assert len(rows) == 128
(base / "reference-address.csv").write_text(result.stdout)
(base / "reference-address.json").write_text(
    json.dumps(
        {
            "source": str(source),
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "function_sha256": hashlib.sha256(function.encode()).hexdigest(),
            "rows": len(rows),
            "scope": "Compiled unmodified registered bitplane_rga_ptmod function; register/address sampling only, not a live raster DMA closure",
        },
        indent=2,
    )
    + "\n"
)
print(len(rows), "registered reference pointer/modulo samples")

# Compile the registered bitplane service branch and its unmodified lane readers.
start = s_source = source.read_text()
readers = start[
    start.index("static uae_u16 fetch16(") : start.index("static uae_u32 fetch32_spr(")
]
readers += start[
    start.index("static uae_u64 fetch64(") : start.index("static void process_copper(")
]
service_start = start.index("if (r->reg >= 0x110 && r->reg < 0x120)")
service = start[
    service_start : start.index("if (r->type & CYCLE_BLITTER)", service_start)
]
pre_service = r"""#include <cstdio>
#include <cstdint>
#include <cstdlib>
using uaecptr=uint32_t;
using uae_u16=uint16_t; using uae_u32=uint32_t; using uae_u64=uint64_t;
constexpr int CYCLE_BITPLANE=1;
struct rgabuf { int reg,type,bpldat,bplmod; uaecptr pv; };
bool aga_mode=true,disinc=false,done=false;
int fetchmode_fmode_bpl,fetchmode_bytes;
uaecptr bplpt[8];
struct { uae_u16 chipset_latch_rw; } regs;
uae_u16 words[4]; int width;
uae_u16 chipmem_wget_indirect(uaecptr p) {
 if(p<0x2000 || p>=0x2008 || (p&1)) std::exit(2);
 return uae_u16(0x1111*((p-0x2000)/2+1));
}
uae_u32 chipmem_lget_indirect(uaecptr p) {
 return (uae_u32(chipmem_wget_indirect(p))<<16)|chipmem_wget_indirect(p+2);
}
void write_drga_dat_bpl16(int,uaecptr,uae_u16 v,int) { width=1; words[0]=v; }
void write_drga_dat_bpl32(int,uaecptr,uae_u32 v,int) { width=2; words[0]=v>>16;words[1]=v; }
void write_drga_dat_bpl64(int,uaecptr,uae_u64 v,int) {
 width=4; for(int i=0;i<4;i++)words[i]=v>>(48-i*16);
}
"""
main_service = (
    r"""int main() {
 int rows=0;
 for(int prior=0;prior<4;prior++)
 for(int current=0;current<4;current++)
 for(int lane=0;lane<4;lane++)
 for(int mod : {-4,6}) {
  rgabuf cell={0x110,CYCLE_BITPLANE,0,mod,uaecptr(0x2000+lane*2)};
  for(int i=0;i<4;i++)words[i]=0;
  fetchmode_fmode_bpl=current;
  fetchmode_bytes=current==0?2:current==3?8:4;
  rgabuf *r=&cell;
"""
    + service
    + r"""
  std::printf("%d,%d,%u,%d,%u,%d,%u,%u,%u,%u\n",
     prior,current,uae_u32(0x2000+lane*2),mod,bplpt[0],width,
     words[0],words[1],words[2],words[3]);
  rows++;
 }
 return rows==128?0:1;
}
"""
)
# initializer_list is needed only by the harness's signed-modulo range.
cpp = base / "reference-service.cpp"
cpp.write_text("#include <initializer_list>\n" + pre_service + readers + main_service)
subprocess.run(
    ["clang++", "-std=c++17", str(cpp), "-o", str(base / "reference-service")],
    check=True,
)
result = subprocess.run(
    [str(base / "reference-service")], capture_output=True, text=True, check=True
)
rows = result.stdout.strip().splitlines()
if len(rows) != 128:
    raise SystemExit(f"service coverage differs: {len(rows)}")
(base / "reference-service.csv").write_text(result.stdout)
(base / "reference-service.json").write_text(
    json.dumps(
        {
            "source_sha256": expected,
            "readers_sha256": hashlib.sha256(readers.encode()).hexdigest(),
            "service_sha256": hashlib.sha256(service.encode()).hexdigest(),
            "rows": len(rows),
            "scope": "Unmodified registered lane readers and bitplane service branch with live FMODE; no live sequencer/raster closure",
        },
        indent=2,
    )
    + "\n"
)
print(len(rows), "registered reference service samples")
