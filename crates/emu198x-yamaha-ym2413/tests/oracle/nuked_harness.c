/*
 * Run an oracle script (see make_cases.py) on Nuked-OPLL and print what the
 * crate's own runner prints: a hash of the two output pins per block of
 * internal clocks. regenerate.sh builds this against Nuked-OPLL at a pinned
 * commit; Nuked-OPLL itself is not part of this repository.
 *
 * Output, one line each:
 *   b INDEX HASH     FNV-1a-64 of the pins over a block of BLOCK clocks
 *   e CLOCKS HASH    the pins over the clocks after the last full block
 */

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "opll.h"

#define BLOCK 4096

static opll_t chip;
static uint64_t hash;
static uint64_t clocks;
static uint64_t block_index;

static void fnv_byte(uint8_t byte)
{
    hash ^= byte;
    hash *= 0x100000001b3ull;
}

static void reset_hash(void)
{
    hash = 0xcbf29ce484222325ull;
}

static void run(uint64_t n)
{
    int32_t pins[2];
    while (n--) {
        int16_t mo, ro;
        OPLL_Clock(&chip, pins);
        mo = (int16_t)pins[0];
        ro = (int16_t)pins[1];
        fnv_byte((uint8_t)(mo & 0xff));
        fnv_byte((uint8_t)((uint16_t)mo >> 8));
        fnv_byte((uint8_t)(ro & 0xff));
        fnv_byte((uint8_t)((uint16_t)ro >> 8));
        clocks++;
        if (clocks % BLOCK == 0) {
            printf("b %llu %016llx\n", (unsigned long long)block_index++,
                   (unsigned long long)hash);
            reset_hash();
        }
    }
}

int main(int argc, char **argv)
{
    char line[256];
    FILE *script;

    if (argc != 2) {
        fprintf(stderr, "usage: %s SCRIPT\n", argv[0]);
        return 2;
    }
    script = fopen(argv[1], "r");
    if (!script) {
        perror(argv[1]);
        return 2;
    }
    reset_hash();
    OPLL_Reset(&chip, opll_type_ym2413);
    while (fgets(line, sizeof line, script)) {
        char word[32];
        char arg[32];
        unsigned a = 0, b = 0;
        if (sscanf(line, "%31s", word) != 1) {
            continue;
        }
        if (!strcmp(word, "variant")) {
            sscanf(line, "%*s %31s", arg);
            OPLL_Reset(&chip, strcmp(arg, "vrc7") ? opll_type_ym2413 : opll_type_ds1001);
        } else if (!strcmp(word, "w")) {
            sscanf(line, "%*s %i %i", &a, &b);
            OPLL_Write(&chip, a, (uint8_t)b);
        } else if (!strcmp(word, "c")) {
            sscanf(line, "%*s %i", &a);
            run(a);
        } else if (!strcmp(word, "reg")) {
            sscanf(line, "%*s %i %i", &a, &b);
            OPLL_Write(&chip, 0, (uint8_t)a);
            run(3);
            OPLL_Write(&chip, 1, (uint8_t)b);
            run(21);
        } else {
            fprintf(stderr, "unknown command: %s", line);
            return 2;
        }
    }
    printf("e %llu %016llx\n", (unsigned long long)(clocks % BLOCK), (unsigned long long)hash);
    return 0;
}
