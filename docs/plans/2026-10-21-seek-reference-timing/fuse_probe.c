/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Unmodified Fuse controller/drive. Authored register/event host only. */
#include "config.h"
#include "upd_fdc.h"
static void send(upd_fdc *f, unsigned b) {
    assert((upd_fdc_read_status(f)&0xc0)==0x80);
    upd_fdc_write_data(f,b);
}
int main(int argc,char **argv) {
    assert(argc==2); char *end; unsigned srt=strtoul(argv[1],&end,10);
    assert(!*end && srt<16);
    fdd_register_startup(); fdd_t d={0}; const fdd_params_t params={1,1,40};
    assert(fdd_init(&d,FDD_SHUGART,&params,0)==FDD_OK);
    d.ready=1; // Explicit supplied ready mechanism; no media transfer claim.
    upd_fdc *f=upd_fdc_alloc_fdc(UPD765A,UPD_CLOCK_4MHZ);
    f->drive[0]=&d;upd_fdc_master_reset(f);
    send(f,3);send(f,srt<<4);send(f,3);
    assert(f->intrq==UPD_INTRQ_NONE);
    const unsigned targets[]={1,3,1,1};
    for(unsigned c=0;c<4;c++) {
      unsigned old=d.c_cylinder,start=tstates;
      printf("{\"srt\":%u,\"case\":%u,\"from\":%u,\"target\":%u,\"host_hz\":3546895,\"steps\":[",srt,c,old,targets[c]);
      send(f,15);send(f,0);send(f,targets[c]);
      unsigned count=0;
      if((unsigned)d.c_cylinder!=old) { printf("{\"tick\":0,\"physical\":%d,\"pcn\":%d}",d.c_cylinder,f->pcn[0]);old=d.c_cylinder;count++; }
      while(f->intrq==UPD_INTRQ_NONE) {
        assert(tstates-start<3546895);probe_advance();
        if((unsigned)d.c_cylinder!=old) {
          printf("%s{\"tick\":%u,\"physical\":%d,\"pcn\":%d}",count?",":"",tstates-start,d.c_cylinder,f->pcn[0]);old=d.c_cylinder;count++;
        }
      }
      unsigned elapsed=tstates-start;
      send(f,8);unsigned st0=upd_fdc_read_data(f),pcn=upd_fdc_read_data(f);
      assert(st0==32 && pcn==targets[c] && f->intrq==UPD_INTRQ_NONE);
      printf("],\"completion_ticks\":%u,\"st0\":%u,\"pcn\":%u,\"idle_msr\":%u}\n",elapsed,st0,pcn,upd_fdc_read_status(f));
    }
    free(f);return 0;
}
