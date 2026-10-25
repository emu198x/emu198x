// Run an oracle script through ymfm's ADPCM-A and ADPCM-B engines and print
// what they produce, in the format tests/ymfm_oracle.rs prints.
//
// ymfm is by Aaron Giles, BSD-3-Clause. regenerate.sh fetches it at a pinned
// commit; it is never part of this repository.
//
// Each sample step follows ymfm's ym2610::clock_fm_and_adpcm: ADPCM-A runs
// on every third step, ADPCM-B on every step, then both are mixed.

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <vector>

#include "ymfm_adpcm.h"

namespace {

class rom_interface : public ymfm::ymfm_interface {
public:
    std::vector<uint8_t> rom;

    uint8_t ymfm_external_read(ymfm::access_class, uint32_t address) override {
        return address < rom.size() ? rom[address] : 0;
    }
};

constexpr uint64_t BLOCK = 1024;
constexpr uint64_t FNV_OFFSET = 0xcbf29ce484222325ull;

struct runner {
    rom_interface intf;
    ymfm::adpcm_a_engine a{intf, 8};
    ymfm::adpcm_b_engine b{intf, 8};
    unsigned phase = 0;
    uint64_t hash = FNV_OFFSET;
    uint64_t samples = 0;
    uint64_t block_index = 0;

    void fnv(uint8_t byte) {
        hash ^= byte;
        hash *= 0x100000001b3ull;
    }

    void fnv32(int32_t value) {
        uint32_t bits = uint32_t(value);
        for (int i = 0; i < 4; i++)
            fnv(uint8_t(bits >> (8 * i)));
    }

    void step() {
        uint8_t ended = 0;
        if (++phase == 3) {
            phase = 0;
            ended = uint8_t(a.clock(0x3f));
        }
        b.clock();
        ymfm::ymfm_output<2> out;
        out.data[0] = out.data[1] = 0;
        a.output(out, 0x3f);
        b.output(out, 1);
        fnv32(out.data[0]);
        fnv32(out.data[1]);
        fnv(ended);
        fnv(b.status());
        if (++samples % BLOCK == 0) {
            std::printf("b %llu %016llx\n", (unsigned long long)block_index++,
                        (unsigned long long)hash);
            hash = FNV_OFFSET;
        }
    }
};

} // namespace

int main(int argc, char **argv) {
    if (argc != 2) {
        std::fprintf(stderr, "usage: %s SCRIPT\n", argv[0]);
        return 2;
    }
    FILE *script = std::fopen(argv[1], "r");
    if (!script) {
        std::perror(argv[1]);
        return 1;
    }
    runner r;
    r.a.reset();
    r.b.reset();
    char line[256];
    while (std::fgets(line, sizeof line, script)) {
        char cmd[16];
        unsigned long x = 0, y = 0;
        int n = std::sscanf(line, "%15s %li %li", cmd, (long *)&x, (long *)&y);
        if (n < 1)
            continue;
        if (!std::strcmp(cmd, "rom")) {
            r.intf.rom.resize(y);
            uint32_t state = uint32_t(x);
            for (auto &byte : r.intf.rom) {
                state = state * 1103515245u + 12345u;
                byte = uint8_t(state >> 16);
            }
        } else if (!std::strcmp(cmd, "a")) {
            r.a.write(uint32_t(x), uint8_t(y));
        } else if (!std::strcmp(cmd, "b")) {
            uint8_t data = uint8_t(y);
            // The YM2610 wires ADPCM-B to external memory and never records.
            if (x == 0)
                data = (data | 0x20) & ~0x40;
            r.b.write(uint32_t(x), data);
        } else if (!std::strcmp(cmd, "s")) {
            for (unsigned long i = 0; i < x; i++)
                r.step();
        } else {
            std::fprintf(stderr, "unknown command %s\n", cmd);
            return 1;
        }
    }
    std::printf("e %llu %016llx\n", (unsigned long long)(r.samples % BLOCK),
                (unsigned long long)r.hash);
    return 0;
}
