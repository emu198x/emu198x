#define GB_INTERNAL
#include <Core/gb.h>
#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>

static uint32_t pixels[160 * 144];
static unsigned frames;
static unsigned callbacks;
static int anchored;
static const char *out;
static uint32_t gray(GB_gameboy_t *gb, uint8_t r, uint8_t g, uint8_t b) {
    (void)gb; (void)g; (void)b;
    if (r % 85) { fprintf(stderr, "unexpected gray %u\n", r); exit(2); }
    return (255-r)/85;
}
static void vblank(GB_gameboy_t *gb, GB_vblank_type_t type) {
    callbacks++;
    if (type != GB_VBLANK_TYPE_NORMAL_FRAME || !gb->boot_rom_finished) return;
    if (!anchored) { anchored = 1; frames = 0; return; }
    frames++;
    if (frames == 240 || frames == 480 || frames == 720) {
        char path[1024];
        if (snprintf(path, sizeof(path), "%s-%u.gray", out, frames) >= (int)sizeof(path)) exit(2);
        FILE *file = fopen(path, "wb");
        if (!file) { perror(path); exit(2); }
        for (unsigned i = 0; i < 160*144; i++) {
            if (pixels[i] > 3 || fputc((int)pixels[i], file) == EOF) exit(2);
        }
        if (fclose(file)) exit(2);
        printf("frame=%u callbacks=%u pc=%04x div=%02x\n", frames, callbacks, gb->pc, GB_safe_read_memory(gb, 0xFF04));
        fflush(stdout);
    }
}
int main(int argc, char **argv) {
    if (argc != 4) { fprintf(stderr, "ROM BOOT OUTPUT_PREFIX\n"); return 2; }
    out = argv[3];
    GB_gameboy_t gb;
    GB_init(&gb, GB_MODEL_DMG_B);
    GB_set_rgb_encode_callback(&gb, gray);
    GB_set_palette(&gb, &GB_PALETTE_GREY);
    GB_set_pixels_output(&gb, pixels);
    GB_set_vblank_callback(&gb, vblank);
    if (GB_load_boot_rom(&gb, argv[2]) || GB_load_rom(&gb, argv[1])) return 2;
    uint64_t ticks = 0;
    while (frames < 720 && ticks < 8ULL * 1000 * 1000 * 120) ticks += GB_run(&gb);
    int result = frames == 720 ? 0 : 1;
    printf("frames=%u ticks_8mhz=%llu booted=%d\n", frames, (unsigned long long)ticks, gb.boot_rom_finished);
    GB_free(&gb);
    return result;
}
