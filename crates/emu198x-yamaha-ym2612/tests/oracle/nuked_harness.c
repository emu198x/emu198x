/*
 * Run an oracle script (see make_cases.py) on Nuked-OPN2 and print what the
 * crate's own runner prints: a hash of the output pins per block of clocks,
 * and every value read. regenerate.sh builds this against Nuked-OPN2 at a
 * pinned commit; Nuked-OPN2 itself is not part of this repository.
 *
 * Output, one line each:
 *   r INDEX VALUE    a read, in order
 *   b INDEX HASH     FNV-1a-64 of the pins over a block of BLOCK clocks
 *   e CLOCKS HASH    the pins over the clocks after the last full block
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "ym3438.h"

#define BLOCK 4096

static ym3438_t chip;
static uint64_t hash;
static uint64_t clocks;
static uint64_t block_index;
static uint64_t reads;

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
    Bit16s pins[2];
    while (n--) {
        OPN2_Clock(&chip, pins);
        fnv_byte((uint8_t)(pins[0] & 0xff));
        fnv_byte((uint8_t)((uint16_t)pins[0] >> 8));
        fnv_byte((uint8_t)(pins[1] & 0xff));
        fnv_byte((uint8_t)((uint16_t)pins[1] >> 8));
        fnv_byte((uint8_t)OPN2_ReadIRQPin(&chip));
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
    Bit32u type = 0;

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
    /* The chip type is a global in Nuked-OPN2: read both header lines first. */
    while (fgets(line, sizeof line, script)) {
        char word[32];
        char arg[32];
        unsigned a = 0, b = 0, c = 0;
        if (sscanf(line, "%31s", word) != 1) {
            continue;
        }
        if (!strcmp(word, "variant")) {
            sscanf(line, "%*s %31s", arg);
            if (!strcmp(arg, "ym2612")) {
                type |= ym3438_mode_ym2612;
            }
        } else if (!strcmp(word, "readmode")) {
            sscanf(line, "%*s %31s", arg);
            if (!strcmp(arg, "all")) {
                type |= ym3438_mode_readmode;
            }
            OPN2_SetChipType(type);
            OPN2_Reset(&chip);
        } else if (!strcmp(word, "w")) {
            sscanf(line, "%*s %i %i", &a, &b);
            OPN2_Write(&chip, a, (Bit8u)b);
        } else if (!strcmp(word, "c")) {
            sscanf(line, "%*s %i", &a);
            run(a);
        } else if (!strcmp(word, "r")) {
            sscanf(line, "%*s %i", &a);
            printf("r %llu %u\n", (unsigned long long)reads++, OPN2_Read(&chip, a));
        } else if (!strcmp(word, "pin")) {
            sscanf(line, "%*s %i", &a);
            OPN2_SetTestPin(&chip, a);
        } else if (!strcmp(word, "reg")) {
            sscanf(line, "%*s %i %i %i", &a, &b, &c);
            OPN2_Write(&chip, a * 2, (Bit8u)b);
            run(1);
            OPN2_Write(&chip, a * 2 + 1, (Bit8u)c);
            run(32);
        } else {
            fprintf(stderr, "unknown command: %s", line);
            return 2;
        }
    }
    printf("e %llu %016llx\n", (unsigned long long)(clocks % BLOCK), (unsigned long long)hash);
    return 0;
}
