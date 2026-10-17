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
    for (unsigned id = 1; id <= 2; id++) {
        unsigned char header[] = {0xfe, 0, 0, (unsigned char)id, 2};
        unsigned char data[513]; data[0] = 0xfb; memset(data + 1, 0x20 + id, 512);
        gap(12, 0); field(header, sizeof header); gap(22, 0x4e); gap(12, 0);
        field(data, sizeof data); gap(42, 0x4e);
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
    assert(argc >= 2 && argc <= 4);
    char *end; unsigned long host = strtoul(argv[1], &end, 0);
    assert(!*end && host <= 255);
    fdd_register_startup();
    create_track();
    if (argc == 4) {
        assert(strcmp(argv[3], "bad-crc") == 0);
        /* First data CRC byte: the payload remains a complete exact match. */
        raw[3 + 652] ^= 1;
        memcpy(original, raw, sizeof raw);
    }
    if (argc >= 3) {
        FILE *file = fopen(argv[2], "wb"); assert(file);
        assert(fwrite(raw, 1, sizeof raw, file) == sizeof raw);
        assert(fclose(file) == 0);
    }
    fdd_t drive = {0};
    const fdd_params_t params = {1, 1, 40};
    assert(fdd_init(&drive, FDD_SHUGART, &params, 0) == FDD_OK);
    /* Ready raw-track fixture, no disk-container/ROM-loading claim. */
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
    const unsigned char command[] = {0x51, 0, 0, 0, 1, 2, 2, 0x2a, 1};
    for (size_t i = 0; i < sizeof command; i++) { wait_request(fdc, 0x80); upd_fdc_write_data(fdc, command[i]); }
    unsigned sent = 0, deadline = tstates + 2 * machine_current->timings.processor_speed;
    while (fdc->state != UPD_FDC_STATE_RES) {
        assert(tstates < deadline && sent < 2048);
        unsigned msr = upd_fdc_read_status(fdc);
        if ((msr & 0xe0) == 0xa0) { upd_fdc_write_data(fdc, (unsigned char)host); sent++; }
        probe_advance();
    }
    unsigned result[7];
    for (int i = 0; i < 7; i++) { wait_request(fdc, 0xc0); result[i] = upd_fdc_read_data(fdc); }
    assert((upd_fdc_read_status(fdc) & 0xf0) == 0x80);
    assert(memcmp(raw, original, sizeof raw) == 0);
    printf("{\"host\":%lu,\"sent\":%u,\"result\":[%u,%u,%u,%u,%u,%u,%u],\"idle_msr\":%u,\"tstates\":%u,\"media_unchanged\":true}\n",
           host, sent, result[0], result[1], result[2], result[3], result[4], result[5], result[6], upd_fdc_read_status(fdc), tstates);
    free(fdc); return 0;
}
