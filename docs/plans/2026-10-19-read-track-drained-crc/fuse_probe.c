/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Public register probe; real upstream controller, drive and CRC paths. */
#include "config.h"
#include "upd_fdc.h"
#include "bitmap.h"
#include "crc.h"

static unsigned char raw[3 + 6250 + 3 * 782];
static unsigned char *track = raw + 3;
static unsigned char original[sizeof raw];
static size_t pos;
static unsigned ids[2], count, kind, position;
static size_t crc_offsets[2], id_offsets[2];
static void put(unsigned byte, int missing_clock) {
    assert(pos < 6250);
    track[pos] = byte;
    if (missing_clock) bitmap_set(track + 6250, pos);
    pos++;
}
static void gap(unsigned count, unsigned byte) { while (count--) put(byte, 0); }
static void field(const unsigned char *bytes, size_t count) {
    libspectrum_word crc = 0xffff;
    for (int i = 0; i < 3; i++) { put(0xa1, 1); crc = crc_fdc(crc, 0xa1); }
    for (size_t i = 0; i < count; i++) { put(bytes[i], 0); crc = crc_fdc(crc, bytes[i]); }
    put(crc >> 8, 0); put(crc & 255, 0);
}
static void create_track(void) {
    raw[0] = 6250 & 255; raw[1] = 6250 >> 8; raw[2] = 0;
    gap(80, 0x4e);
    for (unsigned i = 0; i < count; i++) {
        unsigned id = ids[i];
        unsigned char header[] = {0xfe, 0, 0, (unsigned char)id, 2};
        unsigned char data[513]; data[0] = 0xfb; memset(data + 1, 0x20 + id, 512);
        gap(12, 0); field(header, sizeof header); id_offsets[i] = pos - 2; gap(22, 0x4e); gap(12, 0);
        field(data, sizeof data); crc_offsets[i] = pos - 2; gap(42, 0x4e);
    }
    gap(6250 - pos, 0x4e);
    memcpy(original, raw, sizeof raw);
}
static void wait_request(upd_fdc *fdc, unsigned direction) {
    unsigned deadline = tstates + 2 * machine_current->timings.processor_speed;
    while ((upd_fdc_read_status(fdc) & 0xc0) != direction) {
        assert(tstates < deadline); probe_advance();
    }
}
int main(int argc, char **argv) {
    assert(argc == 6);
    char *end;
    unsigned order = (unsigned)strtoul(argv[1], &end, 0); assert(!*end && order <= 2);
    count = order == 0 ? 1 : 2;
    ids[0] = order == 2 ? 2 : 1; ids[1] = order == 2 ? 1 : 2;
    kind = (unsigned)strtoul(argv[2], &end, 0); assert(!*end && kind <= 2);
    position = (unsigned)strtoul(argv[3], &end, 0); assert(!*end && position < count);
    unsigned repeat = (unsigned)strtoul(argv[4], &end, 0); assert(!*end && repeat >= 1 && repeat <= 2);
    fdd_register_startup(); create_track();
    if (kind == 1) track[id_offsets[position]] ^= 1;
    if (kind == 2) track[crc_offsets[position]] ^= 1;
    memcpy(original, raw, sizeof raw);
    FILE *file = fopen(argv[5], "wb"); assert(file);
    assert(fwrite(raw, 1, sizeof raw, file) == sizeof raw); assert(fclose(file) == 0);
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
    const unsigned char command[] = {0x42, 0, 0, 0, 1, 2, (unsigned char)count, 0x2a, 0xff};
    for (size_t i = 0; i < sizeof command; i++) { wait_request(fdc, 0x80); upd_fdc_write_data(fdc, command[i]); }
    unsigned payload[1024], got = 0, result[7], results = 0;
    unsigned deadline = tstates + 2 * machine_current->timings.processor_speed;
    while (results < 7) {
        assert(tstates < deadline);
        unsigned msr = upd_fdc_read_status(fdc);
        if ((msr & 0xc0) == 0xc0) {
            unsigned byte = upd_fdc_read_data(fdc);
            if (msr & 0x20) { assert(got < count * 512); payload[got++] = byte; }
            else result[results++] = byte;
        }
        probe_advance();
    }
    assert(got == count * 512);
    for (unsigned i = 0; i < got; i++) assert(payload[i] == 0x20 + ids[i / 512]);
    unsigned idle = upd_fdc_read_status(fdc); assert(idle == 0x80);
    wait_request(fdc, 0x80); upd_fdc_write_data(fdc, 4);
    wait_request(fdc, 0x80); upd_fdc_write_data(fdc, 0);
    wait_request(fdc, 0xc0); unsigned recovery = upd_fdc_read_data(fdc);
    assert(upd_fdc_read_status(fdc) == 0x80);
    assert(memcmp(raw, original, sizeof raw) == 0);
    printf("{\"repeat\":%u,\"ids\":[%u", repeat, ids[0]);
    if(count == 2) printf(",%u", ids[1]);
    printf("],\"kind\":%u,\"position\":%u,\"command\":[66,0,0,0,1,2,%u,42,255],\"data\":[",kind,position,count);
    for(unsigned i=0;i<got;i++) printf("%s%u",i ? "," : "",payload[i]);
    printf("],\"result\":[%u,%u,%u,%u,%u,%u,%u],\"idle_msr\":%u,\"recovery\":%u,\"recovery_msr\":128,\"tstates\":%u,\"media_unchanged\":true}\n",result[0],result[1],result[2],result[3],result[4],result[5],result[6],idle,recovery,tstates);
    free(fdc); return 0;
}
