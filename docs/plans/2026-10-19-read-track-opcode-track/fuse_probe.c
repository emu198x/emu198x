/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Public register probe; real upstream controller, drive and CRC paths. */
#include "config.h"
#include "upd_fdc.h"
#include "bitmap.h"
#include "crc.h"

static unsigned char raw[3 + 6250 + 3 * 782];
static unsigned char *track = raw + 3;
static unsigned char original[sizeof raw];
static void wait_request(upd_fdc *fdc, unsigned direction) {
    unsigned deadline = tstates + 2 * machine_current->timings.processor_speed;
    while ((upd_fdc_read_status(fdc) & 0xc0) != direction) {
        assert(tstates < deadline); probe_advance();
    }
}
int main(int argc, char **argv) {
    assert(argc == 11);
    unsigned char command[9];
    for(int i=0;i<9;i++) { char *end; unsigned long n=strtoul(argv[i+2], &end, 10); assert(!*end && n<256); command[i]=(unsigned char)n; }
    FILE *file=fopen(argv[1],"rb"); assert(file);
    size_t n=fread(raw,1,sizeof raw,file); assert(n==sizeof raw); int closed=fclose(file); assert(closed==0);
    memcpy(original,raw,sizeof raw);
    fdd_register_startup();
    fdd_t drive = {0};
    const fdd_params_t params = {1, 1, 40};
    assert(fdd_init(&drive, FDD_SHUGART, &params, 0) == FDD_OK);
    /* Authored ready raw-track fixture, not ROM or motor-start qualification. */
    drive.disk.sides = 1; drive.disk.cylinders = 1;
    drive.disk.bpt = drive.disk.c_bpt = 6250;
    drive.disk.data = raw; drive.disk.tlen = sizeof raw;
    drive.disk.track = track; drive.disk.clocks = track + 6250;
    drive.disk.fm = drive.disk.clocks + 782; drive.disk.weak = drive.disk.fm + 782;
    drive.loaded = drive.motoron = drive.ready = 1; drive.wrprot = 0;
    upd_fdc *fdc = upd_fdc_alloc_fdc(UPD765A, UPD_CLOCK_8MHZ);
    fdc->drive[0] = &drive; upd_fdc_master_reset(fdc);
    const unsigned char specify[] = {3, 0xdf, 3};
    for (size_t i = 0; i < sizeof specify; i++) { wait_request(fdc, 0x80); upd_fdc_write_data(fdc, specify[i]); }
    unsigned sent=0;
    for (size_t i = 0; i < sizeof command; i++) {
        unsigned deadline=tstates+2*machine_current->timings.processor_speed;
        while(!(upd_fdc_read_status(fdc)&0x80)) { assert(tstates<deadline); probe_advance(); }
        if(upd_fdc_read_status(fdc)&0x40) break; /* early rejection: do not send parameters */
        upd_fdc_write_data(fdc,command[i]); sent++;
    }
    unsigned payload[65536], got = 0, result[7], results = 0;
    unsigned deadline = tstates + 2 * machine_current->timings.processor_speed;
    while (1) {
        assert(tstates < deadline);
        unsigned msr = upd_fdc_read_status(fdc);
        if(results && msr==0x80) break;
        if ((msr & 0xc0) == 0xc0) {
            unsigned byte = upd_fdc_read_data(fdc);
            if (msr & 0x20) { assert(got < 65536); payload[got++] = byte; }
            else { assert(results<7); result[results++] = byte; }
        }
        probe_advance();
    }
    assert((sent==1 && results==1 && result[0]==0x80 && got==0) || (sent==9 && results==7));
    unsigned idle = upd_fdc_read_status(fdc); assert(idle == 0x80);
    wait_request(fdc, 0x80); upd_fdc_write_data(fdc, 4);
    wait_request(fdc, 0x80); upd_fdc_write_data(fdc, 0);
    wait_request(fdc, 0xc0); unsigned recovery = upd_fdc_read_data(fdc);
    assert(upd_fdc_read_status(fdc) == 0x80);
    assert(memcmp(raw, original, sizeof raw) == 0);
    printf("{\"data\":[");
    for(unsigned i=0;i<got;i++) printf("%s%u",i ? "," : "",payload[i]);
    printf("],\"result\":[");
    for(unsigned i=0;i<results;i++) printf("%s%u",i?",":"",result[i]);
    printf("],\"command_bytes_sent\":%u,\"idle_msr\":%u,\"recovery\":%u,\"recovery_msr\":128,\"tstates\":%u,\"media_unchanged\":true}\n",sent,idle,recovery,tstates);
    free(fdc); return 0;
}
