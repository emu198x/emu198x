// Differential oracle: ares's S-DSP, compiled from its own source, run
// against a scripted case. See regenerate.sh.
//
// ares (ISC licence, (c) 2004-2025 ares team, Near et al). The struct below
// is ares's sfc/dsp/dsp.hpp less the scheduler thread, audio node and
// debugger; every member function body is ares's, #included unmodified, and
// DSP::main is extracted from ares's dsp.cpp by regenerate.sh.
//
// Output, one line per 2048 steps: "<steps> <hash>", a running FNV-1a over
// all 128 registers after every step and each sample as it is produced.
// Then "ram <hash>" over the 64 KB of audio RAM.

#include <nall/nall.hpp>
#include <ares/types.hpp>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>
using namespace nall;

struct Stop {};

struct Event {
  u64 at;
  u8 reg;
  u8 data;
};

struct DSP {
  n8 apuram[64_KiB];
  n8 registers[128];

  DSP();

  auto main() -> void;
  auto read(n7 address) -> n8;
  auto write(n7 address, n8 data) -> void;

  // Harness state.
  u64 steps = 0;
  u64 end = 0;
  u64 hash = 0xcbf29ce484222325ull;
  std::vector<Event> events;
  size_t nextEvent = 0;
  std::vector<std::pair<s16, s16>> pending;

  auto fold(u8 byte) -> void {
    hash ^= byte;
    hash *= 0x100000001b3ull;
  }
  auto applyEvents() -> void {
    while(nextEvent < events.size() && events[nextEvent].at == steps) {
      write(events[nextEvent].reg, events[nextEvent].data);
      nextEvent++;
    }
  }

  struct Envelope { enum : u32 {
    Release,
    Attack,
    Decay,
    Sustain,
  };};

  struct Clock {
    n15 counter;
    n1  sample = 1;
  } clock;

  struct MainVol {
    n1  reset = 1;
    n1  mute = 1;
    i8  volume[2];
    i17 output[2];
  } mainvol;

  struct Echo {
    i8  feedback;
    i8  volume[2];
    i8  fir[8];
    i16 history[2][8];
    n8  page;
    n4  delay;
    n1  readonly = 1;
    i17 input[2];
    i17 output[2];

    n8  _page;
    n1  _readonly;
    n16 _address;
    n16 _offset;
    n16 _length;
    n3  _historyOffset;
  } echo;

  struct Noise {
    n5  frequency;
    n15 lfsr = 0x4000;
  } noise;

  struct BRR {
    n8  bank;

    n8  _bank;
    n8  _source;
    n16 _address;
    n16 _nextAddress;
    n8  _header;
    n8  _byte;
  } brr;

  struct Latch {
    n8  adsr0;
    n8  envx;
    n8  outx;
    n15 pitch;
    i16 output;
  } latch;

  struct Voice {
    n7  index;

    i8  volume[2];
    n14 pitch;
    n8  source;
    n8  adsr0;
    n8  adsr1;
    n8  gain;
    n8  envx;
    n1  keyon;
    n1  keyoff;
    n1  modulate;
    n1  noise;
    n1  echo;
    n1  end;

    i16 buffer[12];
    n4  bufferOffset;
    n16 gaussianOffset;
    n16 brrAddress;
    n4  brrOffset = 1;
    n3  keyonDelay;
    n2  envelopeMode;
    n11 envelope;

    i32 _envelope;
    n1  _keylatch;
    n1  _keyon;
    n1  _keyoff;
    n1  _modulate;
    n1  _noise;
    n1  _echo;
    n1  _end;
    n1  _looped;
  } voice[8];

  i16 gaussianTable[512];
  auto gaussianConstructTable() -> void;
  auto gaussianInterpolate(const Voice& v) -> s32;

  static const n16 CounterRate[32];
  static const n16 CounterOffset[32];
  auto counterTick() -> void;
  auto counterPoll(u32 rate) -> bool;

  auto envelopeRun(Voice& v) -> void;
  auto brrDecode(Voice& v) -> void;

  auto misc27() -> void;
  auto misc28() -> void;
  auto misc29() -> void;
  auto misc30() -> void;

  auto voiceOutput(Voice& v, n1 channel) -> void;
  auto voice1 (Voice& v) -> void;
  auto voice2 (Voice& v) -> void;
  auto voice3 (Voice& v) -> void;
  auto voice3a(Voice& v) -> void;
  auto voice3b(Voice& v) -> void;
  auto voice3c(Voice& v) -> void;
  auto voice4 (Voice& v) -> void;
  auto voice5 (Voice& v) -> void;
  auto voice6 (Voice& v) -> void;
  auto voice7 (Voice& v) -> void;
  auto voice8 (Voice& v) -> void;
  auto voice9 (Voice& v) -> void;

  auto calculateFIR(n1 channel, s32 index) -> s32;
  auto echoOutput(n1 channel) const -> i16;
  auto echoRead(n1 channel) -> void;
  auto echoWrite(n1 channel) -> void;
  auto echo22() -> void;
  auto echo23() -> void;
  auto echo24() -> void;
  auto echo25() -> void;
  auto echo26() -> void;
  auto echo27() -> void;
  auto echo28() -> void;
  auto echo29() -> void;
  auto echo30() -> void;

  // One step done: fold the registers and any sample, then apply the
  // writes due before the next step.
  auto tick() -> void {
    for(u32 n = 0; n < 128; n++) fold(registers[n]);
    for(auto [l, r] : pending) {
      fold(u16(l) & 0xff); fold(u16(l) >> 8);
      fold(u16(r) & 0xff); fold(u16(r) >> 8);
    }
    pending.clear();
    steps++;
    if(steps % 2048 == 0) printf("%llu %016llx\n", (unsigned long long)steps, (unsigned long long)hash);
    if(steps == end) throw Stop{};
    applyEvents();
  }

  auto sample(i16 left, i16 right) -> void {
    pending.push_back({(s16)left, (s16)right});
  }
};

#include <sfc/dsp/memory.cpp>
#include <sfc/dsp/gaussian.cpp>
#include <sfc/dsp/counter.cpp>
#include <sfc/dsp/envelope.cpp>
#include <sfc/dsp/brr.cpp>
#include <sfc/dsp/misc.cpp>
#include <sfc/dsp/voice.cpp>
#include <sfc/dsp/echo.cpp>
#include "ares_main.inc"

// ares's power(true), less the thread and audio stream: everything at its
// declared default, voice indices set, the Gaussian table built. RAM and
// registers start clear.
DSP::DSP() {
  for(u32 n = 0; n < 64_KiB; n++) apuram[n] = 0;
  for(u32 n = 0; n < 128; n++) registers[n] = 0;
  for(u32 n = 0; n < 8; n++) voice[n].index = n << 4;
  gaussianConstructTable();
}

static auto hex(const std::string& s) -> u32 { return (u32)strtoul(s.c_str(), nullptr, 16); }

int main(int argc, char** argv) {
  if(argc != 2) { fprintf(stderr, "usage: %s case.dsp\n", argv[0]); return 2; }
  static DSP dsp;
  std::ifstream in(argv[1]);
  std::string line;
  u64 at = 0;
  while(std::getline(in, line)) {
    auto hashAt = line.find('#');
    if(hashAt != std::string::npos) line.resize(hashAt);
    std::istringstream words(line);
    std::string op;
    if(!(words >> op)) continue;
    if(op == "ram") {
      std::string a, b;
      words >> a;
      u32 addr = hex(a);
      while(words >> b) dsp.apuram[(n16)addr++] = hex(b);
    } else if(op == "at") {
      words >> at;
    } else if(op == "w") {
      std::string r, d;
      words >> r >> d;
      dsp.events.push_back({at, (u8)hex(r), (u8)hex(d)});
    } else if(op == "end") {
      words >> dsp.end;
    } else {
      fprintf(stderr, "unknown command %s\n", op.c_str());
      return 2;
    }
  }
  dsp.applyEvents();
  try {
    while(true) dsp.main();
  } catch(Stop) {}
  u64 h = 0xcbf29ce484222325ull;
  for(u32 n = 0; n < 64_KiB; n++) { h ^= (u8)dsp.apuram[n]; h *= 0x100000001b3ull; }
  printf("ram %016llx\n", (unsigned long long)h);
}
