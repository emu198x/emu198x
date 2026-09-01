#include "VAmiga.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>

using namespace vamiga;

namespace {

constexpr u32 READY_BASE = 0x0002FF00;
constexpr u32 READY_MAGIC = 0x53504858;
constexpr u16 READY_SCHEMA_V1 = 1;
constexpr u16 EXPECTED_CASE_NUMBER = 1;
constexpr u32 CHIP_RAM_BYTES = 512 * 1024;
constexpr isize TEXTURE_WIDTH = 912;
constexpr isize TEXTURE_HEIGHT = 313;
constexpr std::size_t TEXTURE_PIXELS =
    static_cast<std::size_t>(TEXTURE_WIDTH * TEXTURE_HEIGHT);
constexpr std::size_t CAPTURE_FIELDS = 3;
constexpr int MAX_BOOT_FIELDS = 600;
constexpr isize SAMPLE_ROW = 132;

constexpr u32 HBLANK_RGB = 0x00444444;
constexpr u32 MARKER_RGB = 0x0000F000;
constexpr u32 SPRITE_RGB = 0x000000F0;

struct ReadyRecord {
    u16 number;
    u16 schema;
    u32 fieldCounter;
    u16 spr0pos;
    u16 spr0ctl;
    u16 spr0data;
    u16 spr0datb;
    u16 diwstrt;
    u16 diwstop;
    u16 ddfstrt;
    u16 ddfstop;
    u16 bplcon0;
    u16 bplcon1;
    u16 bplcon2;
    u16 bplcon3;
    u16 bplcon4;
    u16 fmode;
    u16 color00;
    u16 color01;
    u16 color17;
    u16 dmaconEnable;
    u16 markerWordIndex;
    u16 markerBitIndex;
    u32 bpl1Pointer;
    u32 spr0Pointer;
    u16 spriteDataLines;
    u16 sampleBeamLine;
    std::string identity;
};

struct MessageState {
    std::atomic<unsigned> configMessages = 0;
    std::atomic<bool> cpuHalted = false;
    std::atomic<bool> shutdown = false;
    std::atomic<bool> aborted = false;
};

struct CapturedTexture {
    i64 frame;
    bool lof;
    bool prevlof;
    std::vector<u32> pixels;
};

struct Interval {
    isize start;
    isize stop;

    bool operator==(const Interval &) const = default;
};

struct Observation {
    isize hblankStop;
    Interval marker;
    Interval sprite;

    bool operator==(const Observation &) const = default;
};

void process_message(const void *listener, Message message)
{
    auto &state = *static_cast<MessageState *>(const_cast<void *>(listener));
    switch (message.type) {
        case Msg::CONFIG:
            state.configMessages.fetch_add(1, std::memory_order_relaxed);
            break;
        case Msg::CPU_HALT:
            state.cpuHalted.store(true, std::memory_order_relaxed);
            break;
        case Msg::SHUTDOWN:
            state.shutdown.store(true, std::memory_order_relaxed);
            break;
        case Msg::ABORT:
            state.aborted.store(true, std::memory_order_relaxed);
            break;
        default:
            break;
    }
}

class Session {
public:
    VAmiga machine;

    Session() = default;
    Session(const Session &) = delete;
    Session &operator=(const Session &) = delete;

    ~Session()
    {
        try {
            if (suspended_) {
                machine.resume();
                suspended_ = false;
            }
            if (launched_) {
                machine.halt();
                launched_ = false;
            }
        } catch (...) {
        }
    }

    void launch(MessageState &messages)
    {
        machine.launch(&messages, process_message);
        launched_ = true;
    }

    void suspend()
    {
        if (!suspended_) {
            machine.suspend();
            suspended_ = true;
        }
    }

    void resume()
    {
        if (suspended_) {
            machine.resume();
            suspended_ = false;
        }
    }

    [[nodiscard]] bool isSuspended() const { return suspended_; }

    void halt()
    {
        resume();
        if (launched_) {
            machine.halt();
            launched_ = false;
        }
    }

private:
    bool launched_ = false;
    bool suspended_ = false;
};

[[nodiscard]] u32 read_long(VAmiga &machine, u32 address)
{
    const auto high = machine.mem.debugger.spypeek16(Accessor::CPU, address);
    const auto low = machine.mem.debugger.spypeek16(Accessor::CPU, address + 2);
    return (u32(high) << 16) | low;
}

[[nodiscard]] u16 read_word(VAmiga &machine, u32 address)
{
    return machine.mem.debugger.spypeek16(Accessor::CPU, address);
}

[[nodiscard]] std::string read_ascii(VAmiga &machine, u32 address, std::size_t maximum)
{
    std::string result;
    result.reserve(maximum);
    for (std::size_t offset = 0; offset < maximum; ++offset) {
        const auto byte = machine.mem.debugger.spypeek8(
            Accessor::CPU, address + static_cast<u32>(offset));
        if (byte == 0) {
            return result;
        }
        if (byte < 0x21 || byte > 0x7E) {
            throw std::runtime_error("SPHX identity contains a non-printable byte");
        }
        result.push_back(static_cast<char>(byte));
    }
    throw std::runtime_error("SPHX identity is not NUL-terminated");
}

[[nodiscard]] ReadyRecord read_ready_record(VAmiga &machine)
{
    return ReadyRecord {
        .number = read_word(machine, READY_BASE + 0x04),
        .schema = read_word(machine, READY_BASE + 0x06),
        .fieldCounter = read_long(machine, READY_BASE + 0x08),
        .spr0pos = read_word(machine, READY_BASE + 0x0C),
        .spr0ctl = read_word(machine, READY_BASE + 0x0E),
        .spr0data = read_word(machine, READY_BASE + 0x10),
        .spr0datb = read_word(machine, READY_BASE + 0x12),
        .diwstrt = read_word(machine, READY_BASE + 0x14),
        .diwstop = read_word(machine, READY_BASE + 0x16),
        .ddfstrt = read_word(machine, READY_BASE + 0x18),
        .ddfstop = read_word(machine, READY_BASE + 0x1A),
        .bplcon0 = read_word(machine, READY_BASE + 0x1C),
        .bplcon1 = read_word(machine, READY_BASE + 0x1E),
        .bplcon2 = read_word(machine, READY_BASE + 0x20),
        .bplcon3 = read_word(machine, READY_BASE + 0x22),
        .bplcon4 = read_word(machine, READY_BASE + 0x24),
        .fmode = read_word(machine, READY_BASE + 0x26),
        .color00 = read_word(machine, READY_BASE + 0x28),
        .color01 = read_word(machine, READY_BASE + 0x2A),
        .color17 = read_word(machine, READY_BASE + 0x2C),
        .dmaconEnable = read_word(machine, READY_BASE + 0x2E),
        .markerWordIndex = read_word(machine, READY_BASE + 0x30),
        .markerBitIndex = read_word(machine, READY_BASE + 0x32),
        .bpl1Pointer = read_long(machine, READY_BASE + 0x34),
        .spr0Pointer = read_long(machine, READY_BASE + 0x38),
        .spriteDataLines = read_word(machine, READY_BASE + 0x3C),
        .sampleBeamLine = read_word(machine, READY_BASE + 0x3E),
        .identity = read_ascii(machine, READY_BASE + 0x40, 64),
    };
}

void validate_guest_buffers(VAmiga &machine, const ReadyRecord &record)
{
    constexpr u32 wordsPerRow = 20;
    constexpr u32 rows = 256;
    const auto bitplaneBytes = wordsPerRow * rows * 2;
    const auto spriteWords = 2U + u32(record.spriteDataLines) * 2U + 2U;
    const auto spriteBytes = spriteWords * 2;
    if ((record.bpl1Pointer & 1U) != 0 || record.bpl1Pointer >= CHIP_RAM_BYTES ||
        record.bpl1Pointer + bitplaneBytes > CHIP_RAM_BYTES) {
        throw std::runtime_error("SPHX bitplane pointer is outside chip RAM");
    }
    if ((record.spr0Pointer & 1U) != 0 || record.spr0Pointer >= CHIP_RAM_BYTES ||
        record.spr0Pointer + spriteBytes > CHIP_RAM_BYTES) {
        throw std::runtime_error("SPHX sprite pointer is outside chip RAM");
    }

    for (u32 row = 0; row < rows; ++row) {
        for (u32 word = 0; word < wordsPerRow; ++word) {
            const auto expected = word == record.markerWordIndex
                ? static_cast<u16>(1U << record.markerBitIndex)
                : u16(0);
            const auto address = record.bpl1Pointer + (row * wordsPerRow + word) * 2;
            if (read_word(machine, address) != expected) {
                throw std::runtime_error("guest bitplane does not match the SPHX record");
            }
        }
    }

    if (read_word(machine, record.spr0Pointer) != record.spr0pos ||
        read_word(machine, record.spr0Pointer + 2) != record.spr0ctl) {
        throw std::runtime_error("guest sprite header does not match the SPHX record");
    }
    for (u32 line = 0; line < record.spriteDataLines; ++line) {
        const auto address = record.spr0Pointer + 4 + line * 4;
        if (read_word(machine, address) != record.spr0data ||
            read_word(machine, address + 2) != record.spr0datb) {
            throw std::runtime_error("guest sprite body does not match the SPHX record");
        }
    }
    const auto terminator = record.spr0Pointer + 4 + u32(record.spriteDataLines) * 4;
    if (read_word(machine, terminator) != 0 || read_word(machine, terminator + 2) != 0) {
        throw std::runtime_error("guest sprite terminator is not empty");
    }
}

void validate_ready_record(VAmiga &machine, const ReadyRecord &record)
{
    const bool fixedFieldsMatch =
        record.number == EXPECTED_CASE_NUMBER && record.schema == READY_SCHEMA_V1 &&
        record.spr0pos == 0x8064 && record.spr0ctl == 0x9000 &&
        record.spr0data == 0xFFFF && record.spr0datb == 0x0000 &&
        record.diwstrt == 0x2C81 && record.diwstop == 0xF4C1 &&
        record.ddfstrt == 0x0038 && record.ddfstop == 0x00D0 &&
        record.bplcon0 == 0x1000 && record.bplcon1 == 0x0000 &&
        record.bplcon2 == 0x0000 && record.bplcon3 == 0x0000 &&
        record.bplcon4 == 0x0011 && record.fmode == 0x0000 &&
        record.color00 == 0x0001 && record.color01 == 0x00F0 &&
        record.color17 == 0x0F00 && record.dmaconEnable == 0x8320 &&
        record.markerWordIndex == 4 && record.markerBitIndex == 15 &&
        record.spriteDataLines == 16 && record.sampleBeamLine == SAMPLE_ROW &&
        record.identity == "amiga-sprite-phase-v1/01-fixed-lores-sprite";
    if (!fixedFieldsMatch) {
        throw std::runtime_error("SPHX ready record does not match fixed-lores-sprite");
    }
    validate_guest_buffers(machine, record);
}

void fail_on_runtime_message(const MessageState &messages)
{
    if (messages.cpuHalted.load(std::memory_order_relaxed)) {
        throw std::runtime_error("vAmiga reported a halted CPU");
    }
    if (messages.shutdown.load(std::memory_order_relaxed)) {
        throw std::runtime_error("vAmiga shut down during capture");
    }
    if (messages.aborted.load(std::memory_order_relaxed)) {
        throw std::runtime_error("vAmiga aborted during capture");
    }
}

template<typename Predicate>
void wait_until(
    Session &session,
    const MessageState &messages,
    Predicate predicate,
    std::chrono::milliseconds timeout,
    const char *description)
{
    const auto deadline = std::chrono::steady_clock::now() + timeout;
    session.machine.wakeUp();
    while (!predicate()) {
        fail_on_runtime_message(messages);
        if (std::chrono::steady_clock::now() >= deadline) {
            throw std::runtime_error(std::string("timed out waiting for ") + description);
        }
        std::this_thread::sleep_for(std::chrono::microseconds(100));
    }
}

void set_capture_configuration(Session &session, const MessageState &messages)
{
    auto &machine = session.machine;
    machine.set(ConfigScheme::A500_OCS_1MB);
    machine.set(Opt::AMIGA_VIDEO_FORMAT, i64(TV::PAL));
    machine.set(Opt::CPU_REVISION, i64(CPURev::CPU_68000));
    machine.set(Opt::AGNUS_REVISION, i64(AgnusRevision::OCS));
    machine.set(Opt::DENISE_REVISION, i64(DeniseRev::OCS));
    machine.set(Opt::MEM_CHIP_RAM, 512);
    machine.set(Opt::MEM_SLOW_RAM, 512);
    machine.set(Opt::MEM_FAST_RAM, 0);
    machine.set(Opt::AMIGA_WARP_BOOT, 0);
    machine.set(Opt::AMIGA_WARP_MODE, i64(Warp::NEVER));
    machine.set(Opt::AMIGA_VSYNC, 1);
    machine.set(Opt::AMIGA_SPEED_BOOST, 100);
    machine.set(Opt::AMIGA_RUN_AHEAD, 0);
    machine.set(Opt::HOST_REFRESH_RATE, 50);
    machine.set(Opt::DENISE_FRAME_SKIPPING, 0);
    machine.set(Opt::DENISE_HIDDEN_BITPLANES, 0);
    machine.set(Opt::DENISE_HIDDEN_SPRITES, 0);
    machine.set(Opt::DENISE_HIDDEN_LAYERS, 0);
    machine.set(Opt::DMA_DEBUG_ENABLE, 0);
    machine.set(Opt::MON_PALETTE, i64(Palette::RGB));
    machine.set(Opt::MON_BRIGHTNESS, 50);
    machine.set(Opt::MON_CONTRAST, 100);
    machine.set(Opt::MON_SATURATION, 50);
    machine.set(Opt::VID_WHITE_NOISE, 0);

    const std::vector<std::pair<Opt, i64>> expected {
        {Opt::AMIGA_VIDEO_FORMAT, i64(TV::PAL)},
        {Opt::CPU_REVISION, i64(CPURev::CPU_68000)},
        {Opt::AGNUS_REVISION, i64(AgnusRevision::OCS)},
        {Opt::DENISE_REVISION, i64(DeniseRev::OCS)},
        {Opt::MEM_CHIP_RAM, 512},
        {Opt::MEM_SLOW_RAM, 512},
        {Opt::MEM_FAST_RAM, 0},
        {Opt::AMIGA_WARP_BOOT, 0},
        {Opt::AMIGA_WARP_MODE, i64(Warp::NEVER)},
        {Opt::AMIGA_VSYNC, 1},
        {Opt::AMIGA_SPEED_BOOST, 100},
        {Opt::AMIGA_RUN_AHEAD, 0},
        {Opt::HOST_REFRESH_RATE, 50},
        {Opt::DENISE_FRAME_SKIPPING, 0},
        {Opt::DENISE_HIDDEN_BITPLANES, 0},
        {Opt::DENISE_HIDDEN_SPRITES, 0},
        {Opt::DENISE_HIDDEN_LAYERS, 0},
        {Opt::DMA_DEBUG_ENABLE, 0},
        {Opt::MON_PALETTE, i64(Palette::RGB)},
        {Opt::MON_BRIGHTNESS, 50},
        {Opt::MON_CONTRAST, 100},
        {Opt::MON_SATURATION, 50},
        {Opt::VID_WHITE_NOISE, 0},
    };
    wait_until(
        session,
        messages,
        [&machine, &expected] {
            return std::all_of(
                expected.begin(), expected.end(), [&machine](const auto &entry) {
                    return machine.get(entry.first) == entry.second;
                });
        },
        std::chrono::seconds(2),
        "the vAmiga configuration barrier");
}

[[nodiscard]] i64 current_emulator_frame(Session &session)
{
    if (!session.isSuspended()) {
        throw std::logic_error("frame inspection requires a suspended machine");
    }
    return session.machine.agnus.getInfo().frame;
}

[[nodiscard]] i64 step_one_field(Session &session, const MessageState &messages)
{
    const auto before = current_emulator_frame(session);
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
    session.resume();
    session.machine.wakeUp();
    while (true) {
        std::this_thread::sleep_for(std::chrono::microseconds(100));
        session.suspend();
        const auto after = current_emulator_frame(session);
        if (after == before + 1) {
            fail_on_runtime_message(messages);
            return after;
        }
        if (after > before + 1) {
            throw std::runtime_error("vAmiga advanced by more than one field");
        }
        fail_on_runtime_message(messages);
        if (std::chrono::steady_clock::now() >= deadline) {
            throw std::runtime_error("vAmiga did not complete the requested field");
        }
        session.resume();
    }
}

[[nodiscard]] CapturedTexture copy_stable_texture(Session &session)
{
    if (!session.isSuspended()) {
        throw std::logic_error("texture extraction requires a suspended machine");
    }
    auto &port = session.machine.videoPort;
    port.lockTexture();
    try {
        isize frame = 0;
        bool lof = false;
        bool prevlof = false;
        const auto *source = port.getTexture(&frame, &lof, &prevlof);
        if (source == nullptr) {
            throw std::runtime_error("VideoPortAPI returned a null texture");
        }
        CapturedTexture result {
            .frame = frame,
            .lof = lof,
            .prevlof = prevlof,
            .pixels = std::vector<u32>(source, source + TEXTURE_PIXELS),
        };
        port.unlockTexture();
        return result;
    } catch (...) {
        port.unlockTexture();
        throw;
    }
}

[[nodiscard]] Interval find_single_interval(
    const std::vector<u32> &pixels, std::size_t rowOffset, u32 rgb, const char *name)
{
    std::optional<Interval> result;
    isize sample = 0;
    while (sample < TEXTURE_WIDTH) {
        if ((pixels[rowOffset + static_cast<std::size_t>(sample)] & 0x00FFFFFFU) != rgb) {
            ++sample;
            continue;
        }
        const auto start = sample;
        while (sample < TEXTURE_WIDTH &&
               (pixels[rowOffset + static_cast<std::size_t>(sample)] & 0x00FFFFFFU) == rgb) {
            ++sample;
        }
        if (result.has_value()) {
            throw std::runtime_error(std::string("multiple ") + name + " intervals on sample row");
        }
        result = Interval {.start = start, .stop = sample};
    }
    if (!result.has_value()) {
        throw std::runtime_error(std::string("no ") + name + " interval on sample row");
    }
    return *result;
}

[[nodiscard]] Observation measure_texture(const CapturedTexture &texture)
{
    const auto rowOffset = static_cast<std::size_t>(SAMPLE_ROW * TEXTURE_WIDTH);
    isize hblankStop = 0;
    while (hblankStop < TEXTURE_WIDTH &&
           (texture.pixels[rowOffset + static_cast<std::size_t>(hblankStop)] & 0x00FFFFFFU) ==
               HBLANK_RGB) {
        ++hblankStop;
    }
    if (hblankStop == 0 || hblankStop == TEXTURE_WIDTH) {
        throw std::runtime_error("leading hardwired HBLANK interval is not measurable");
    }
    const auto marker = find_single_interval(texture.pixels, rowOffset, MARKER_RGB, "marker");
    const auto sprite = find_single_interval(texture.pixels, rowOffset, SPRITE_RGB, "sprite");
    if (marker.stop - marker.start != 2) {
        throw std::runtime_error("marker is not one low-resolution pixel wide");
    }
    if (sprite.stop - sprite.start != 32) {
        throw std::runtime_error("sprite is not sixteen low-resolution pixels wide");
    }
    return Observation {
        .hblankStop = hblankStop,
        .marker = marker,
        .sprite = sprite,
    };
}

[[nodiscard]] std::filesystem::path temporary_path(const std::filesystem::path &path)
{
    return path.parent_path() / ("." + path.filename().string() + ".tmp");
}

void replace_with_temporary(
    const std::filesystem::path &temporary,
    const std::filesystem::path &destination)
{
    if (std::filesystem::exists(destination)) {
        throw std::runtime_error("refusing to overwrite " + destination.string());
    }
    std::filesystem::rename(temporary, destination);
}

void write_u32_le(std::ostream &stream, u32 value)
{
    const char bytes[] {
        static_cast<char>(value),
        static_cast<char>(value >> 8),
        static_cast<char>(value >> 16),
        static_cast<char>(value >> 24),
    };
    stream.write(bytes, 4);
}

void write_raw_textures(
    const std::filesystem::path &path,
    const std::vector<CapturedTexture> &textures)
{
    const auto temporary = temporary_path(path);
    std::ofstream stream(temporary, std::ios::binary | std::ios::trunc);
    if (!stream) {
        throw std::runtime_error("cannot create temporary raw-texture file");
    }
    for (const auto &texture : textures) {
        for (const auto pixel : texture.pixels) {
            write_u32_le(stream, pixel);
        }
    }
    stream.close();
    if (!stream) {
        throw std::runtime_error("cannot finish temporary raw-texture file");
    }
    replace_with_temporary(temporary, path);
}

[[nodiscard]] std::string json_escape(const std::string &value)
{
    std::ostringstream result;
    for (const unsigned char character : value) {
        switch (character) {
            case '"': result << "\\\""; break;
            case '\\': result << "\\\\"; break;
            case '\n': result << "\\n"; break;
            case '\r': result << "\\r"; break;
            case '\t': result << "\\t"; break;
            default:
                if (character < 0x20) {
                    result << "\\u" << std::hex << std::setw(4) << std::setfill('0')
                           << static_cast<unsigned>(character);
                } else {
                    result << static_cast<char>(character);
                }
        }
    }
    return result.str();
}

void write_i64_array(std::ostream &stream, const std::vector<i64> &values)
{
    stream << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index != 0) stream << ", ";
        stream << values[index];
    }
    stream << ']';
}

void write_u32_array(std::ostream &stream, const std::vector<u32> &values)
{
    stream << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index != 0) stream << ", ";
        stream << values[index];
    }
    stream << ']';
}

void write_bool_array(std::ostream &stream, const std::vector<bool> &values)
{
    stream << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index != 0) stream << ", ";
        stream << (values[index] ? "true" : "false");
    }
    stream << ']';
}

void write_result(
    const std::filesystem::path &path,
    const ReadyRecord &ready,
    i64 readyEmulatorFrame,
    const std::vector<i64> &emulatorFrames,
    const std::vector<u32> &guestFields,
    const std::vector<CapturedTexture> &textures,
    const std::vector<Observation> &observations)
{
    const auto temporary = temporary_path(path);
    std::ofstream stream(temporary, std::ios::trunc);
    if (!stream) {
        throw std::runtime_error("cannot create temporary adapter result");
    }
    std::vector<i64> textureFrames;
    std::vector<bool> textureLof;
    std::vector<bool> texturePrevLof;
    for (const auto &texture : textures) {
        textureFrames.push_back(texture.frame);
        textureLof.push_back(texture.lof);
        texturePrevLof.push_back(texture.prevlof);
    }

    stream << "{\n";
    stream << "  \"schema_version\": \"1.0.0\",\n";
    stream << "  \"producer_version\": \"" << json_escape(VAmiga::version()) << "\",\n";
    stream << "  \"producer_build\": \"" << json_escape(VAmiga::build()) << "\",\n";
    stream << "  \"case_id\": \"fixed-lores-sprite\",\n";
    stream << "  \"ready_emulator_frame\": " << readyEmulatorFrame << ",\n";
    stream << "  \"ready_record\": {\n";
    stream << "    \"case_number\": " << ready.number << ",\n";
    stream << "    \"schema_version\": " << ready.schema << ",\n";
    stream << "    \"field_counter\": " << ready.fieldCounter << ",\n";
    stream << "    \"spr0pos\": " << ready.spr0pos << ",\n";
    stream << "    \"spr0ctl\": " << ready.spr0ctl << ",\n";
    stream << "    \"spr0data\": " << ready.spr0data << ",\n";
    stream << "    \"spr0datb\": " << ready.spr0datb << ",\n";
    stream << "    \"diwstrt\": " << ready.diwstrt << ",\n";
    stream << "    \"diwstop\": " << ready.diwstop << ",\n";
    stream << "    \"ddfstrt\": " << ready.ddfstrt << ",\n";
    stream << "    \"ddfstop\": " << ready.ddfstop << ",\n";
    stream << "    \"bplcon0\": " << ready.bplcon0 << ",\n";
    stream << "    \"bplcon1\": " << ready.bplcon1 << ",\n";
    stream << "    \"bplcon2\": " << ready.bplcon2 << ",\n";
    stream << "    \"bplcon3\": " << ready.bplcon3 << ",\n";
    stream << "    \"bplcon4\": " << ready.bplcon4 << ",\n";
    stream << "    \"fmode\": " << ready.fmode << ",\n";
    stream << "    \"color00\": " << ready.color00 << ",\n";
    stream << "    \"color01\": " << ready.color01 << ",\n";
    stream << "    \"color17\": " << ready.color17 << ",\n";
    stream << "    \"dmacon_enable\": " << ready.dmaconEnable << ",\n";
    stream << "    \"marker_word_index\": " << ready.markerWordIndex << ",\n";
    stream << "    \"marker_bit_index\": " << ready.markerBitIndex << ",\n";
    stream << "    \"bpl1_pointer\": " << ready.bpl1Pointer << ",\n";
    stream << "    \"spr0_pointer\": " << ready.spr0Pointer << ",\n";
    stream << "    \"sprite_data_lines\": " << ready.spriteDataLines << ",\n";
    stream << "    \"sample_beam_line\": " << ready.sampleBeamLine << ",\n";
    stream << "    \"identity\": \"" << json_escape(ready.identity) << "\"\n";
    stream << "  },\n";
    stream << "  \"captured_emulator_frames\": ";
    write_i64_array(stream, emulatorFrames);
    stream << ",\n  \"captured_guest_fields\": ";
    write_u32_array(stream, guestFields);
    stream << ",\n  \"captured_texture_frames\": ";
    write_i64_array(stream, textureFrames);
    stream << ",\n  \"texture_lof\": ";
    write_bool_array(stream, textureLof);
    stream << ",\n  \"texture_prevlof\": ";
    write_bool_array(stream, texturePrevLof);
    stream << ",\n";
    stream << "  \"texture_width\": " << TEXTURE_WIDTH << ",\n";
    stream << "  \"texture_height\": " << TEXTURE_HEIGHT << ",\n";
    stream << "  \"texture_count\": " << textures.size() << ",\n";
    stream << "  \"pixel_count\": " << textures.size() * TEXTURE_PIXELS << ",\n";
    stream << "  \"observations\": [\n";
    for (std::size_t index = 0; index < observations.size(); ++index) {
        const auto &observation = observations[index];
        stream << "    {\"hblank_stop\": " << observation.hblankStop
               << ", \"marker_start\": " << observation.marker.start
               << ", \"marker_stop\": " << observation.marker.stop
               << ", \"sprite_start\": " << observation.sprite.start
               << ", \"sprite_stop\": " << observation.sprite.stop << '}';
        stream << (index + 1 == observations.size() ? "\n" : ",\n");
    }
    stream << "  ]\n";
    stream << "}\n";
    stream.close();
    if (!stream) {
        throw std::runtime_error("cannot finish temporary adapter result");
    }
    replace_with_temporary(temporary, path);
}

void export_configuration(VAmiga &machine, const std::filesystem::path &path)
{
    const auto temporary = temporary_path(path);
    machine.exportConfig(temporary, false);
    replace_with_temporary(temporary, path);
}

} // namespace

int main(int argc, char **argv)
{
    if (argc != 6) {
        std::cerr << "usage: vamiga-sprite-phase-capture ROM ADF RAW CONFIG RESULT\n";
        return 64;
    }

    try {
        const auto rawPath = std::filesystem::path(argv[3]);
        const auto configPath = std::filesystem::path(argv[4]);
        const auto resultPath = std::filesystem::path(argv[5]);
        for (const auto &path : {rawPath, configPath, resultPath}) {
            if (std::filesystem::exists(path)) {
                throw std::runtime_error("refusing to overwrite " + path.string());
            }
        }

        MessageState messages;
        Session session;
        session.launch(messages);
        set_capture_configuration(session, messages);
        export_configuration(session.machine, configPath);

        session.machine.mem.loadRom(std::filesystem::path(argv[1]));
        session.machine.df0.insert(std::filesystem::path(argv[2]), true);
        session.machine.powerOn();
        session.machine.run();
        wait_until(
            session,
            messages,
            [&session] { return session.machine.isRunning(); },
            std::chrono::seconds(2),
            "the running state");
        session.suspend();

        ReadyRecord ready {};
        i64 readyEmulatorFrame = 0;
        bool settled = false;
        for (int field = 0; field < MAX_BOOT_FIELDS; ++field) {
            const auto emulatorFrame = step_one_field(session, messages);
            if (read_long(session.machine, READY_BASE) == READY_MAGIC) {
                ready = read_ready_record(session.machine);
                validate_ready_record(session.machine, ready);
                if (ready.fieldCounter >= 8) {
                    readyEmulatorFrame = emulatorFrame;
                    settled = true;
                    break;
                }
            }
        }
        if (!settled) {
            throw std::runtime_error("SPHX probe did not settle within 600 fields");
        }

        std::vector<i64> emulatorFrames;
        std::vector<u32> guestFields;
        std::vector<CapturedTexture> textures;
        std::vector<Observation> observations;
        for (std::size_t field = 0; field < CAPTURE_FIELDS; ++field) {
            const auto emulatorFrame = step_one_field(session, messages);
            if (read_long(session.machine, READY_BASE) != READY_MAGIC) {
                throw std::runtime_error("SPHX magic disappeared during capture");
            }
            const auto current = read_ready_record(session.machine);
            validate_ready_record(session.machine, current);
            const auto expectedField = ready.fieldCounter + static_cast<u32>(field) + 1;
            if (current.fieldCounter != expectedField) {
                throw std::runtime_error("SPHX field counter is not consecutive");
            }
            auto texture = copy_stable_texture(session);
            auto observation = measure_texture(texture);
            emulatorFrames.push_back(emulatorFrame);
            guestFields.push_back(current.fieldCounter);
            textures.push_back(std::move(texture));
            observations.push_back(observation);
        }

        for (std::size_t index = 1; index < textures.size(); ++index) {
            if (textures[index].frame != textures[index - 1].frame + 1) {
                throw std::runtime_error("VideoPortAPI texture frames are not adjacent");
            }
            if (textures[index].pixels != textures[0].pixels) {
                throw std::runtime_error("adjacent stable textures differ");
            }
            if (!(observations[index] == observations[0])) {
                throw std::runtime_error("adjacent texture measurements differ");
            }
        }

        fail_on_runtime_message(messages);
        session.halt();
        write_raw_textures(rawPath, textures);
        write_result(
            resultPath,
            ready,
            readyEmulatorFrame,
            emulatorFrames,
            guestFields,
            textures,
            observations);

        std::cout << "captured case=fixed-lores-sprite ready_field="
                  << ready.fieldCounter << " texture_frames=" << textures.front().frame
                  << ',' << textures.back().frame << " hblank_stop="
                  << observations.front().hblankStop << " marker=["
                  << observations.front().marker.start << ','
                  << observations.front().marker.stop << ") sprite=["
                  << observations.front().sprite.start << ','
                  << observations.front().sprite.stop << ")\n";
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "error: " << error.what() << '\n';
        return 1;
    }
}
