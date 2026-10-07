#include <cstdint>
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
struct rgabuf *write_rga(int slot, int type, uae_u16 v, uae_u32 *p)
{
	struct rgabuf *r = &rga_pipe[(slot + rga_slot_first_offset) & 3];

	bool strobe = (v >= 0x38 && v < 0x40) || v == 0x1fe;

	if (r->reg != 0x1fe && !strobe) {
		write_log("RGA conflict: %04x -> %04x, %08x | %08x -> %08x, %04x, %d\n",
			r->reg, v,
			p ? *p : 0, r->pv, (p ? *p : 0) | r->pv, 
			v,
			slot);
	}
	// RGA bus address conflict causes AND operation
	r->reg &= v;
	r->type |= type;
	r->alloc = 1;
	if (p && r->p) {
		// DMA address pointer conflict causes both old and new address to becomes old OR new.
		r->conflict = r->p;
		*r->p |= *p;
		*p = *r->p;
		r->pv |= *p;
	} else if (p) {
		r->p = p;
		r->pv = *p;
	}
	return r;
}
int main() {
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
		if (r->type & (CYCLE_REFRESH | CYCLE_STROBE)) {
			*r->p += ref_ras_add;
			*r->p &= refmask;
			refptr = *r->p;
			disinc = true;
#ifdef DEBUGGER
			if (debug_dma) {
				record_dma_read(r->reg, *r->p, DMARECORD_REFRESH, (hp - 3) / 2);
				record_dma_read_value(0xffff);
				int num = r->refdat;
				if (num == 1 && lof_store) {
					record_dma_event(DMA_EVENT_LOF);
				}
				if (num == 1 && lol) {
					record_dma_event(DMA_EVENT_LOL);
				}
			}
#endif
			done = true;
		}
		// BPL
		if (r->reg >= 0x110 && r->reg < 0x120) {

			int num = r->bpldat & 7;
			uaecptr pt = r->pv;

#ifdef DEBUGGER
			if (debug_dma) {
				record_dma_read(r->reg, pt, DMARECORD_BITPLANE, num);
				if (r->bplmod) {
					record_dma_event(DMA_EVENT_MODADD);
				}
			}
			if (memwatch_enabled) {
				debug_getpeekdma_chipram(pt, MW_MASK_BPL_0 + num, r->reg);
			}
#endif
			if (!aga_mode) {
				uae_u32 dat = fetch16(r);
				write_drga_dat_bpl16(r->reg, pt, dat, num);
				regs.chipset_latch_rw = (uae_u16)dat;
			} else {
				if (fetchmode_fmode_bpl == 0) {
					uae_u32 dat = fetch16(r);
					write_drga_dat_bpl16(r->reg, pt, dat, num);
					regs.chipset_latch_rw = (uae_u16)dat;
				} else if (fetchmode_fmode_bpl == 1) {
					uae_u32 dat = fetch32_bpl(r);
					write_drga_dat_bpl32(r->reg, pt, dat, num);
					regs.chipset_latch_rw = (uae_u16)dat;
				} else {
					uae_u64 dat64 = fetch64(r);
					write_drga_dat_bpl64(r->reg, pt, dat64, num);
					regs.chipset_latch_rw = (uae_u16)dat64;
				}
			}
			if (!disinc) {
				r->pv += fetchmode_bytes + r->bplmod;
			}
			bplpt[num] = r->pv;
			done = true;
		} else if (r->type & CYCLE_BITPLANE) {
			int num = r->bpldat & 7;
			r->pv += fetchmode_bytes + r->bplmod;
			bplpt[num] = r->pv;
		}

  std::printf("%u,%u,%u,%u,%u,%u,%u,%u,%d,%d\n",chip,mode,plane,fixed,r->reg,captured,refptr,bplpt[plane],reads,payloads);
  count++;
 }
 return count==200?0:2;
}
