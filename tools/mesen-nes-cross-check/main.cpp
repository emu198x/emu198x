// Oracle harness: run a blargg NES test ROM under Mesen2 and print what it
// reports, so Emu198x's own result can be diffed against a passing emulator.
//
// Links against Mesen2's InteropDLL (built with `make core`) and drives it
// through the same C API the Avalonia UI uses. Nothing in the vendored tree is
// modified; this file lives in scratch.
//
// blargg's test shell writes its result to the cartridge RAM the CPU sees at
// $6000: $6001-$6003 hold the signature DE B0 61 once the protocol is live,
// $6000 holds the status ($80 = still running), and $6004 onwards holds a
// zero-terminated ASCII report. MemoryType::NesMemory is the CPU address
// space, so all of it is readable without knowing the mapper's layout.

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <thread>

extern "C" {
int32_t LoadScript(char *name, char *path, char *content, int32_t scriptId);
void GetScriptLog(int32_t scriptId, char *outScriptLog, uint32_t maxLength);
void InitDll();
void InitializeEmu(const char *homeFolder, void *windowHandle, void *viewerHandle,
                   bool softwareRenderer, bool noAudio, bool noVideo, bool noInput);
bool LoadRom(char *filename, char *patchFile);
void SetEmulationFlag(int flag, bool enabled);
bool IsRunning();
void Stop();
void Release();
uint8_t GetMemoryValue(int type, uint32_t address);
void GetMemoryValues(int type, uint32_t start, uint32_t end, uint8_t *output);
}

// MemoryType::NesMemory — index 8 in Core/Shared/MemoryType.h.
static const int NES_MEMORY = 8;
// EmulationFlags::MaximumSpeed — Core/Shared/SettingTypes.h.
static const int MAXIMUM_SPEED = 0x04;

// Read a Lua script to run alongside the ROM, or an empty string if
// EMU198X_MESEN_SCRIPT is unset. The script logs through `emu.log`, which the
// runner drains after the ROM settles.
static std::string read_script() {
  const char *path = getenv("EMU198X_MESEN_SCRIPT");
  if (!path) {
    return "";
  }
  FILE *f = fopen(path, "rb");
  if (!f) {
    fprintf(stderr, "cannot open script %s\n", path);
    return "";
  }
  std::string out;
  char buf[4096];
  size_t n;
  while ((n = fread(buf, 1, sizeof(buf), f)) > 0) {
    out.append(buf, n);
  }
  fclose(f);
  return out;
}

int main(int argc, char *argv[]) {
  if (argc < 3) {
    fprintf(stderr, "usage: mesen_probe <home-folder> <rom> [rom...]\n");
    return 2;
  }

  InitDll();
  InitializeEmu(argv[1], nullptr, nullptr, true, true, true, true);
  SetEmulationFlag(MAXIMUM_SPEED, true);

  for (int i = 2; i < argc; i++) {
    std::string rom = argv[i];
    printf("\n═══ %s ═══\n", rom.c_str());
    fflush(stdout);

    char patch[1] = {0};
    if (!LoadRom(const_cast<char *>(rom.c_str()), patch)) {
      printf("LoadRom failed\n");
      continue;
    }

    // The script has to be loaded after the ROM: the script manager lives on
    // the debugger, which is created for the running console.
    std::string script = read_script();
    int32_t scriptId = -1;
    if (!script.empty()) {
      char name[] = "trace";
      char path[1] = {0};
      scriptId = LoadScript(name, path, const_cast<char *>(script.c_str()), -1);
    }

    // Poll rather than run a fixed frame count: these ROMs settle in well
    // under a second at maximum speed, but a fixed wait would either truncate
    // a slow ROM or waste time on a fast one.
    uint8_t status = 0x80;
    bool sawSignature = false;
    for (int tick = 0; tick < 600; tick++) {
      std::this_thread::sleep_for(std::chrono::milliseconds(100));
      if (GetMemoryValue(NES_MEMORY, 0x6001) == 0xDE &&
          GetMemoryValue(NES_MEMORY, 0x6002) == 0xB0 &&
          GetMemoryValue(NES_MEMORY, 0x6003) == 0x61) {
        sawSignature = true;
        status = GetMemoryValue(NES_MEMORY, 0x6000);
        if (status != 0x80) {
          break;
        }
      }
    }

    if (!sawSignature) {
      printf("no $6000 protocol signature — ROM may not use blargg's shell\n");
    } else {
      printf("$6000 = 0x%02X\n", status);
    }

    uint8_t text[0x800];
    GetMemoryValues(NES_MEMORY, 0x6004, 0x6004 + sizeof(text) - 1, text);
    text[sizeof(text) - 1] = 0;
    printf("%s\n", reinterpret_cast<char *>(text));

    if (scriptId >= 0) {
      std::string log(1 << 20, '\0');
      GetScriptLog(scriptId, &log[0], static_cast<uint32_t>(log.size()));
      printf("──── script log ────\n%s\n", log.c_str());
    }
    fflush(stdout);

    Stop();
  }

  Release();
  return 0;
}
