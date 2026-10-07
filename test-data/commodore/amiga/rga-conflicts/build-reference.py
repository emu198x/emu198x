import argparse
import hashlib
import json
import subprocess
from pathlib import Path

parser = argparse.ArgumentParser(
    description="Compile registered RGA conflict fragments without editing the reference checkout"
)
parser.add_argument("--source", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
source = args.source
base = args.output
base.mkdir(parents=True, exist_ok=True)
expected = "75bb300ba2798913997f82dde988db465746e9b7f22699bbc275690c8ae227e5"
if hashlib.sha256(source.read_bytes()).hexdigest() != expected:
    raise SystemExit("unregistered custom.cpp")
s = source.read_text()
a = s.index("struct rgabuf *write_rga(")
b = s.index("\nSTATIC_INLINE rgabuf *read_rga_out", a)
write = s[a:b]
a = s.index(
    "\t\tif (r->type & (CYCLE_REFRESH | CYCLE_STROBE))",
    s.index("static void handle_rga_out"),
)
b = s.index("\n\t\tswitch (r->reg)", a)
refresh = s[a:b]
a = s.index("\t\t// BPL\n", a)
b = s.index("\n\t\tif (r->type & CYCLE_BLITTER)", a)
bpl = s[a:b]
prelude = """#include <cstdint>
#include <cstdio>
#include <initializer_list>
using uae_u16=uint16_t; using uae_u32=uint32_t; using uae_u64=uint64_t; using uaecptr=uint32_t;
constexpr int CYCLE_BITPLANE=1,CYCLE_REFRESH=2,CYCLE_STROBE=4;
struct rgabuf { uae_u16 reg=0x1fe; int type=0,alloc=0,bpldat=0,bplmod=0; uae_u32 *p=nullptr,*conflict=nullptr,pv=0; };
rgabuf rga_pipe[4]; int rga_slot_first_offset=0;
void write_log(const char*,...) {}
uaecptr refptr,bplpt[8]; unsigned ref_ras_add,refmask;
bool aga_mode; int fetchmode_fmode_bpl,fetchmode_bytes,reads,payloads;
struct { uae_u16 chipset_latch_rw; } regs;
uae_u32 fetch16(rgabuf*) { reads++; return 0xabcd; }
uae_u32 fetch32_bpl(rgabuf*) { reads++; return 0xabcdabcd; }
uae_u64 fetch64(rgabuf*) { reads++; return 0xabcdabcdabcdabcdULL; }
void write_drga_dat_bpl16(uae_u16,uaecptr,uae_u32,int) { payloads++; }
void write_drga_dat_bpl32(uae_u16,uaecptr,uae_u32,int) { payloads++; }
void write_drga_dat_bpl64(uae_u16,uaecptr,uae_u64,int) { payloads++; }
"""
main = (
    """int main() {
 std::puts("chip,mode,plane,fixed_register,combined_register,address,refresh_after,bitplane_after,reads,payloads");
 unsigned count=0;
 for(unsigned chip=0;chip<3;chip++) for(unsigned mode=0;mode<(chip==2?3u:1u);mode++)
 for(unsigned plane=0;plane<8;plane++) for(unsigned fixed: {0x38u,0x3au,0x3cu,0x3eu,0x1feu}) {
  aga_mode=chip==2; fetchmode_fmode_bpl=mode; fetchmode_bytes=2u<<mode;
  refmask=chip==2?0x1fffff:chip==1?0xfffff:0x7ffff;
  ref_ras_add=chip==2?0:chip==1?0x200:2;
  // One preceding ordinary refresh has retired from the reset pointer.
  refptr=chip==2?0x1ffffe:ref_ras_add;
  rga_pipe[0]={}; bplpt[plane]=0x30000; reads=payloads=0;
  auto *r=write_rga(0,CYCLE_BITPLANE,0x110+2*plane,nullptr);
  r->bpldat=plane|8;
  write_rga(0,CYCLE_REFRESH,fixed,&refptr);
  if(r->type==CYCLE_BITPLANE || r->p!=&refptr) return 1;
  const uaecptr captured=r->pv;
  bool done=false,disinc=false;
"""
    + refresh
    + bpl
    + """
  std::printf("%u,%u,%u,%u,%u,%u,%u,%u,%d,%d\\n",chip,mode,plane,fixed,r->reg,captured,refptr,bplpt[plane],reads,payloads);
  count++;
 }
 return count==200?0:2;
}
"""
)
cpp = base / "reference-service.cpp"
cpp.write_text(prelude + write + main)
binary = base / "reference-service"
subprocess.run(
    ["clang++", "-std=c++17", "-O2", str(cpp), "-o", str(binary)], check=True
)
out = subprocess.run([str(binary)], capture_output=True, text=True, check=True)
assert len(out.stdout.splitlines()) == 201
(base / "reference-service.csv").write_text(out.stdout)
mut = base / "reference-service-negative.cpp"
mut.write_text(
    cpp.read_text().replace(
        "  write_rga(0,CYCLE_REFRESH,fixed,&refptr);",
        "  // Negative control drops refresh.",
    )
)
mb = binary.with_name("rga-reference-service-negative")
subprocess.run(["clang++", "-std=c++17", "-O2", str(mut), "-o", str(mb)], check=True)
negative = subprocess.run([str(mb)], capture_output=True, text=True, check=False)
assert negative.returncode != 0
(base / "verification.json").write_text(
    json.dumps(
        {
            "rows": 200,
            "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "fragment_sha256": {
                k: hashlib.sha256(v.encode()).hexdigest()
                for k, v in {
                    "write_rga": write,
                    "refresh_service": refresh,
                    "bitplane_service": bpl,
                }.items()
            },
            "negative_exit": negative.returncode,
            "boundary": "Compiled registered RGA and outgoing refresh/bitplane fragments; stubbed memory payloads; no live raster or silicon claim",
        },
        indent=2,
    )
    + "\n"
)
print("PASS: 200 compiled reference service rows; dropped refresh rejected")

start = s.index("\t\t// SPR\n", s.index("static void handle_rga_out"))
sprite = s[start : s.index("\n\t\t// BPL\n", start)]
sprite_prelude = prelude.replace(
    "CYCLE_STROBE=4", "CYCLE_STROBE=4,CYCLE_SPRITE=8"
).replace("bpldat=0,bplmod=0", "bpldat=0,bplmod=0,sprdat=0")
sprite_prelude += """
struct sprite { uaecptr pt; } spr[8];
int fetchmode_fmode_spr,sprite_width;
uae_u32 fetch32_spr(rgabuf*) { reads++; return 0xabcdabcd; }
void write_drga_dat_spr(uae_u16,uaecptr,uae_u32) { payloads++; }
void write_drga_dat_spr_wide(uae_u16,uaecptr,uae_u64) { payloads++; }
void SPRxCTL_DMA(uae_u16,int) {}
void SPRxPOS(uae_u16,int) {}
"""
sprite_main = (
    """int main() {
 std::puts("chip,mode,channel,control,second,fixed_register,combined_register,address,refresh_after,sprite_after,reads,payloads");
 unsigned count=0;
 for(unsigned chip=0;chip<3;chip++) for(unsigned mode=0;mode<(chip==2?3u:1u);mode++)
 for(unsigned channel=0;channel<8;channel++) for(unsigned control=0;control<2;control++)
 for(unsigned second=0;second<2;second++)
 for(unsigned fixed: {0x38u,0x3au,0x3cu,0x3eu,0x1feu}) {
  fetchmode_fmode_spr=mode==2?3:mode; sprite_width=16u<<mode;
  refmask=chip==2?0x1fffff:chip==1?0xfffff:0x7ffff;
  ref_ras_add=chip==2?0:chip==1?0x200:2;
  refptr=chip==2?0x1ffffe:ref_ras_add;
  rga_pipe[0]={}; spr[channel].pt=0x30000; reads=payloads=0;
  auto *r=write_rga(0,CYCLE_SPRITE,0x140+8*channel+(control?0:4)+2*second,nullptr);
  r->sprdat=channel+(second?8:0)+(control?0:16);
  r->p=&spr[channel].pt; r->pv=spr[channel].pt;
  write_rga(0,CYCLE_REFRESH,fixed,&refptr);
  if(r->type==CYCLE_SPRITE || r->p!=&spr[channel].pt) return 1;
  const uaecptr captured=r->pv;
  bool done=false,disinc=false;
"""
    + refresh
    + sprite
    + """
  std::printf("%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%d,%d\\n",chip,mode,channel,control,second,fixed,r->reg,captured,refptr,spr[channel].pt,reads,payloads);
  count++;
 }
 return count==800?0:2;
}
"""
)
sprite_cpp = base / "reference-sprite.cpp"
sprite_cpp.write_text(sprite_prelude + write + sprite_main)
sprite_binary = base / "reference-sprite"
subprocess.run(
    ["clang++", "-std=c++17", "-O2", str(sprite_cpp), "-o", str(sprite_binary)],
    check=True,
)
sprite_out = subprocess.run(
    [str(sprite_binary)], capture_output=True, text=True, check=True
)
assert len(sprite_out.stdout.splitlines()) == 801
(base / "reference-sprite.csv").write_text(sprite_out.stdout)
(base / "verification-sprite.json").write_text(
    json.dumps(
        {
            "rows": 800,
            "source_sha256": expected,
            "fragment_sha256": {
                name: hashlib.sha256(fragment.encode()).hexdigest()
                for name, fragment in {
                    "write_rga": write,
                    "refresh_service": refresh,
                    "sprite_service": sprite,
                }.items()
            },
            "boundary": "Compiled registered RGA and outgoing refresh/sprite fragments; stubbed memory payloads; no live raster or silicon claim",
        },
        indent=2,
    )
    + "\n"
)
print("PASS: 800 compiled reference sprite service rows")
