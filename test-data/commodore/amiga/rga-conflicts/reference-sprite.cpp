#include <cstdint>
#include <cstdio>
#include <initializer_list>
using uae_u16=uint16_t; using uae_u32=uint32_t; using uae_u64=uint64_t; using uaecptr=uint32_t;
constexpr int CYCLE_BITPLANE=1,CYCLE_REFRESH=2,CYCLE_STROBE=4,CYCLE_SPRITE=8;
struct rgabuf { uae_u16 reg=0x1fe; int type=0,alloc=0,bpldat=0,bplmod=0,sprdat=0; uae_u32 *p=nullptr,*conflict=nullptr,pv=0; };
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

struct sprite { uaecptr pt; } spr[8];
int fetchmode_fmode_spr,sprite_width;
uae_u32 fetch32_spr(rgabuf*) { reads++; return 0xabcdabcd; }
void write_drga_dat_spr(uae_u16,uaecptr,uae_u32) { payloads++; }
void write_drga_dat_spr_wide(uae_u16,uaecptr,uae_u64) { payloads++; }
void SPRxCTL_DMA(uae_u16,int) {}
void SPRxPOS(uae_u16,int) {}
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
		// SPR
		if (r->reg >= 0x140 && r->reg < 0x180) {
			int num = r->sprdat & 7;
			bool slot = (r->sprdat & 8) != 0;
			bool dmastate = (r->sprdat & 0x10) != 0;
			struct sprite *s = &spr[num];
			uae_u16 sdat = 0;
			uaecptr pt = s->pt;

#ifdef DEBUGGER
			if (debug_dma) {
				record_dma_read(r->reg, *r->p, DMARECORD_SPRITE, num);
			}
			if (memwatch_enabled) {
				debug_getpeekdma_chipram(*r->p, MW_MASK_SPR_0 + num, r->reg);
			}
#endif
			if (fetchmode_fmode_spr == 0) {
				uae_u16 dat = fetch16(r);
				if (!dmastate) {
					write_drga_dat_spr(r->reg, pt, dat);
				} else {
					write_drga_dat_spr(r->reg, pt, dat << 16);
				}
				sdat = dat;
			} else if (fetchmode_fmode_spr < 3) {
				uae_u32 dat = fetch32_spr(r);
				sdat = dat >> 16;
				if (!dmastate) {
					write_drga_dat_spr(r->reg, pt, sdat);
				} else {
					write_drga_dat_spr(r->reg, pt, dat);
				}
			} else {
				uae_u64 dat = fetch64(r);
				sdat = dat >> 48;
				if (!dmastate) {
					write_drga_dat_spr(r->reg, pt, sdat);
				} else {
					write_drga_dat_spr_wide(r->reg, pt, dat);
				}
			}

			if (!dmastate) {
				if (slot) {
					SPRxCTL_DMA(sdat, num);
				} else {
					SPRxPOS(sdat, num);
				}
			}
			if (!disinc) {
				r->pv += sprite_width / 8;
			}
			regs.chipset_latch_rw = sdat;
			s->pt = r->pv;
			done = true;
		} else if (r->type & CYCLE_SPRITE) {
			int num = r->sprdat & 7;
			struct sprite *s = &spr[num];
			*r->p += sprite_width / 8;
			s->pt = *r->p;
		}

  std::printf("%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%d,%d\n",chip,mode,channel,control,second,fixed,r->reg,captured,refptr,spr[channel].pt,reads,payloads);
  count++;
 }
 return count==800?0:2;
}
