// Differential oracle for emu198x-nintendo-s-ppu: ares's accurate SNES PPU,
// compiled from its own sources with the rest of the emulator stubbed out.
//
// ares (https://github.com/ares-emulator/ares) is ISC-licensed. This file
// includes its ppu/*.cpp unmodified, except for one patch regenerate.sh
// applies to a copy: in Mode 4 a vertical offset-per-tile entry adds the BG3
// word (as bsnes does) rather than the unfetched vertical word.
//
// The harness reads a case script (see make_cases.py for the format) and
// prints, in order: each scripted read, a hash of the whole picture at the
// end of each frame, and per-row hashes of the last frame's non-blank rows.
// The Rust test `tests/ares_oracle.rs` runs the same scripts and must print
// the same lines.
//
// Power-on randomness is replaced by zero, which is the power-on state the
// Rust model documents.

#include <nall/nall.hpp>
#include <ares/types.hpp>

#include <cstdio>
#include <cstring>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

using namespace nall;

namespace ares::SuperFamicom {

struct Thread {};

namespace Region {
  inline bool pal = false;
  inline auto PAL() -> bool { return pal; }
  inline auto NTSC() -> bool { return !pal; }
}

namespace Node {
  using Object = void*;
  namespace Video {
    struct ScreenImpl {
      std::vector<u32> buffer = std::vector<u32>(564 * 576, 0);
      auto pixels() -> std::vector<u32>& { return buffer; }
      auto setProgressive(bool) -> void {}
      auto setInterlace(bool) -> void {}
      auto setScale(f64, f64) -> void {}
      auto overscan() -> bool { return false; }
      auto setSize(u32, u32) -> void {}
      auto setViewport(u32, u32, u32, u32) -> void {}
      auto frame() -> void {}
    };
    using Screen = ScreenImpl*;
  }
  namespace Setting {
    struct NaturalImpl { u32 v = 0; auto value() -> u32 { return v; } };
    using Natural = NaturalImpl*;
    struct BooleanImpl { bool v = false; auto value() -> bool { return v; } };
    using Boolean = BooleanImpl*;
  }
  namespace Debugger {
    struct Memory {};
    struct Graphics {};
    struct Properties {};
  }
}

struct RandomStub {
  auto operator()() -> u64 { return 0; }
  auto bias(u64) -> u64 { return 0; }
};
inline RandomStub random;

struct CPUStub {
  n8 wrio = 0xff;
  template<typename T> auto synchronize(T&) -> void {}
  auto pio() -> n8 { return wrio; }
};
inline CPUStub cpu;

struct Event { enum : u32 { Frame }; };
auto onFrame() -> void;
struct SchedulerStub { auto exit(u32) -> void { onFrame(); } };
inline SchedulerStub scheduler;

#include <sfc/ppu/counter/counter.hpp>
#include "ppu.hpp"

PPUBase ppu;
PPU ppuImpl;
#define ppu ppuImpl

#include <sfc/ppu/counter/inline.hpp>

auto onStep() -> void;

inline auto PPU::step() -> void {
  tick(2);
  onStep();
}

inline auto PPU::step(u32 clocks) -> void {
  clocks >>= 1;
  while(clocks--) {
    tick(2);
    onStep();
  }
}

#include "main.cpp"
#include "io.cpp"
#include "mosaic.cpp"
#include "background.cpp"
#include "mode7.cpp"
#include "oam.cpp"
#include "object.cpp"
#include "window.cpp"
#include "dac.cpp"

auto PPU::load(Node::Object) -> void {}
auto PPU::unload() -> void {}
auto PPU::map() -> void {}
auto PPU::serialize(serializer&) -> void {}

// ares's PPU::power with the random source returning zero and the settings
// at their defaults (PPU1 version 1, PPU2 version 3, 64 KB VRAM).
auto PPU::power(bool reset) -> void {
  PPUcounter::reset();
  memset(vram.data, 0, sizeof(vram.data));
  ppu1.version = 1;
  ppu1.mdr = 0;
  ppu2.version = 3;
  ppu2.mdr = 0;
  vram.mask = 0x7fff;
  for(auto& object : oam.object) object = {};
  for(auto& word : cgram) word = 0;
  latch = {};
  io = {};
  io.displayDisable = true;
  io.vramIncrementSize = 1;
  mosaic.power();
  bg1.power();
  bg2.power();
  bg3.power();
  bg4.power();
  obj.power();
  window.power();
  dac.power();
  updateVideoMode();
}

}  // namespace ares::SuperFamicom

using namespace ares::SuperFamicom;

struct Command {
  std::string kind;
  u32 frame = 0, v = 0, h = 0;
  u32 reg = 0, value = 0;
};

static std::vector<Command> events;
static size_t nextEvent = 0;
static u32 framesDone = 0;
static u32 framesWanted = 0;
static std::vector<std::string> output;

static auto fnv(u64 hash, u32 value) -> u64 {
  for(u32 i = 0; i < 4; i++) {
    hash ^= (value >> (8 * i)) & 0xff;
    hash *= 0x100000001b3ull;
  }
  return hash;
}

static constexpr u64 basis = 0xcbf29ce484222325ull;

// The case PRNG: xorshift64*, top 16 bits.
struct Prng {
  u64 x;
  explicit Prng(u64 seed) : x(seed | 1) {}
  auto next() -> u16 {
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    return (u16)((x * 0x2545f4914f6cdd1dull) >> 48);
  }
};

// The ares screen row that holds the model's framebuffer row `row`.
static auto pixel(u32 row, u32 x) -> u32 {
  u32 yScale = ppuImpl.state.interlace ? 2 : 1;
  u32 aresRow = row + (Region::PAL() ? 20 * yScale : 0);
  u32 offset = Region::PAL() ? 22 : 26;
  return ppuImpl.screen->buffer[aresRow * 564 + offset + x];
}

auto ares::SuperFamicom::onFrame() -> void {
  u64 hash = basis;
  for(u32 row = 0; row < 480; row++) {
    for(u32 x = 0; x < 512; x++) hash = fnv(hash, pixel(row, x));
  }
  char line[64];
  snprintf(line, sizeof line, "frame %u %016llx", framesDone, (unsigned long long)hash);
  output.push_back(line);
  framesDone++;
  if(framesDone == framesWanted) {
    u64 blank = basis;
    for(u32 x = 0; x < 512; x++) blank = fnv(blank, 0);
    for(u32 row = 0; row < 480; row++) {
      u64 rowHash = basis;
      for(u32 x = 0; x < 512; x++) rowHash = fnv(rowHash, pixel(row, x));
      if(rowHash == blank) continue;
      snprintf(line, sizeof line, "row %u %016llx", row, (unsigned long long)rowHash);
      output.push_back(line);
    }
  }
}

static auto applyEvents() -> void {
  while(nextEvent < events.size()) {
    auto& e = events[nextEvent];
    if(e.frame != framesDone || e.v != ppuImpl.vcounter() || e.h != ppuImpl.hcounter()) return;
    nextEvent++;
    if(e.kind == "w") {
      ppuImpl.writeIO(0x2100 | e.reg, e.value);
    } else if(e.kind == "r") {
      u32 result = ppuImpl.readIO(0x2100 | e.reg, e.value);
      char line[96];
      snprintf(line, sizeof line, "r %u %u %u %02x -> %02x", e.frame, e.v, e.h, e.reg, result & 0xff);
      output.push_back(line);
    } else if(e.kind == "latch") {
      if(cpu.wrio.bit(7) && !(e.value & 1)) ppuImpl.latchCounters();
      cpu.wrio = e.value ? 0xff : 0x7f;
    }
  }
}

auto ares::SuperFamicom::onStep() -> void { applyEvents(); }

int main(int argc, char** argv) {
  if(argc != 2) { fprintf(stderr, "usage: ppu_harness case.ppu\n"); return 2; }
  std::ifstream in(argv[1]);
  std::string text;
  ppuImpl.screen = new Node::Video::ScreenImpl;
  std::vector<std::string> pokes;
  while(std::getline(in, text)) {
    std::istringstream line(text);
    std::string kind;
    if(!(line >> kind) || kind[0] == '#') continue;
    if(kind == "region") {
      std::string region;
      line >> region;
      Region::pal = region == "pal";
    } else if(kind == "frames") {
      line >> framesWanted;
    } else if(kind == "w" || kind == "r" || kind == "latch") {
      Command c;
      c.kind = kind;
      line >> std::dec >> c.frame >> c.v >> c.h;
      if(kind == "latch") {
        line >> c.value;
      } else {
        line >> std::hex >> c.reg >> c.value;
      }
      events.push_back(c);
    } else {
      pokes.push_back(text);
    }
  }
  ppuImpl.power(false);
  for(auto& text : pokes) {
    std::istringstream line(text);
    std::string kind;
    u64 seed = 0;
    u32 address = 0, count = 0, value = 0;
    line >> kind >> std::hex;
    if(kind == "vrand" || kind == "crand") line >> seed;
    line >> address;
    if(kind == "vram") { line >> value; ppuImpl.vram.data[address & 0x7fff] = value; }
    if(kind == "cgram") { line >> value; ppuImpl.dac.cgram[address & 0xff] = value & 0x7fff; }
    if(kind == "oam") { line >> value; ppuImpl.oam.write(address & 0x3ff, value); }
    if(kind == "vrand") {
      line >> count;
      Prng rng(seed);
      for(u32 i = 0; i < count; i++) ppuImpl.vram.data[(address + i) & 0x7fff] = rng.next();
    }
    if(kind == "crand") {
      line >> count;
      Prng rng(seed);
      for(u32 i = 0; i < count; i++) ppuImpl.dac.cgram[(address + i) & 0xff] = rng.next() & 0x7fff;
    }
  }
  applyEvents();
  while(framesDone < framesWanted) ppuImpl.main();
  for(auto& line : output) printf("%s\n", line.c_str());
  if(nextEvent < events.size()) printf("unconsumed %zu\n", events.size() - nextEvent);
  return 0;
}
