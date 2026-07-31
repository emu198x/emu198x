#include "VAmiga.h"

#include <algorithm>
#include <atomic>
#include <bit>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <limits>
#include <sstream>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>

using namespace vamiga;

namespace {

constexpr u32 READY_BASE = 0x0002FF00;
constexpr u32 READY_MAGIC = 0x50415544;
constexpr u16 READY_SCHEMA_V1 = 1;
constexpr u32 EXPECTED_SAMPLE_ADDRESS = 0x000300F0;
constexpr i64 SAMPLE_RATE_HZ = 48'000;
constexpr i64 AUDIO_BUFFER_FRAMES = 8'192;
constexpr int MAX_BOOT_FIELDS = 600;

struct ExpectedCase {
    std::string id;
    u16 number;
    u16 channel;
    u16 period;
    u16 volume;
    u16 sampleWord;
    u16 sampleWords;
    std::string identity;
};

struct ReadyRecord {
    u16 number;
    u16 schema;
    u32 fieldCounter;
    u16 channel;
    u16 period;
    u16 volume;
    u16 sampleWord;
    u16 sampleWords;
    u16 dmacon;
    u32 sampleAddress;
    std::string identity;
};

struct MessageState {
    std::atomic<unsigned> configMessages = 0;
    std::atomic<unsigned> audioUnderflows = 0;
    std::atomic<unsigned> audioOverflows = 0;
    std::atomic<bool> cpuHalted = false;
    std::atomic<bool> shutdown = false;
    std::atomic<bool> aborted = false;
};

void process_message(const void *listener, Message message)
{
    auto &state = *static_cast<MessageState *>(const_cast<void *>(listener));
    switch (message.type) {
        case Msg::CONFIG:
            state.configMessages.fetch_add(1, std::memory_order_relaxed);
            break;
        case Msg::AUDBUF_UNDERFLOW:
            state.audioUnderflows.fetch_add(1, std::memory_order_relaxed);
            break;
        case Msg::AUDBUF_OVERFLOW:
            state.audioOverflows.fetch_add(1, std::memory_order_relaxed);
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

[[nodiscard]] u16 parse_u16(const char *text, const char *name)
{
    std::size_t consumed = 0;
    const auto value = std::stoul(text, &consumed, 0);
    if (text[consumed] != '\0' || value > std::numeric_limits<u16>::max()) {
        throw std::runtime_error(std::string("invalid ") + name + ": " + text);
    }
    return static_cast<u16>(value);
}

[[nodiscard]] u32 read_long(VAmiga &machine, u32 address)
{
    const auto high = machine.mem.debugger.spypeek16(Accessor::CPU, address);
    const auto low = machine.mem.debugger.spypeek16(Accessor::CPU, address + 2);
    return (u32(high) << 16) | low;
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
            throw std::runtime_error("ready identity contains a non-printable byte");
        }
        result.push_back(static_cast<char>(byte));
    }
    throw std::runtime_error("ready identity is not NUL-terminated");
}

[[nodiscard]] ReadyRecord read_ready_record(VAmiga &machine)
{
    return ReadyRecord {
        .number = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x04),
        .schema = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x06),
        .fieldCounter = read_long(machine, READY_BASE + 0x08),
        .channel = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x0C),
        .period = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x0E),
        .volume = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x10),
        .sampleWord = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x12),
        .sampleWords = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x14),
        .dmacon = machine.mem.debugger.spypeek16(Accessor::CPU, READY_BASE + 0x16),
        .sampleAddress = read_long(machine, READY_BASE + 0x18),
        .identity = read_ascii(machine, READY_BASE + 0x20, 64),
    };
}

void validate_ready_record(
    VAmiga &machine,
    const ReadyRecord &record,
    const ExpectedCase &expected)
{
    const auto expectedDmacon = static_cast<u16>(0x8200 | (1U << expected.channel));
    if (record.number != expected.number || record.schema != READY_SCHEMA_V1 ||
        record.channel != expected.channel || record.period != expected.period ||
        record.volume != expected.volume || record.sampleWord != expected.sampleWord ||
        record.sampleWords != expected.sampleWords || record.dmacon != expectedDmacon ||
        record.identity != expected.identity) {
        throw std::runtime_error("PAUD ready record does not match the selected case");
    }
    if (record.sampleAddress != EXPECTED_SAMPLE_ADDRESS ||
        (record.sampleAddress & 1U) != 0) {
        std::ostringstream message;
        message << "unexpected sample address 0x" << std::hex << record.sampleAddress;
        throw std::runtime_error(message.str());
    }

    for (u32 index = 0; index < record.sampleWords; ++index) {
        const auto word = machine.mem.debugger.spypeek16(
            Accessor::CPU, record.sampleAddress + index * 2);
        if (word != expected.sampleWord) {
            throw std::runtime_error("guest sample buffer does not match the ready record");
        }
    }
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
    if (messages.audioUnderflows.load(std::memory_order_relaxed) != 0) {
        throw std::runtime_error("vAmiga reported an audio-buffer underflow");
    }
    if (messages.audioOverflows.load(std::memory_order_relaxed) != 0) {
        throw std::runtime_error("vAmiga reported an audio-buffer overflow");
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

void set_capture_configuration(Session &session, MessageState &messages)
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
    machine.set(Opt::HOST_SAMPLE_RATE, SAMPLE_RATE_HZ);

    machine.set(Opt::AUD_BUFFER_SIZE, AUDIO_BUFFER_FRAMES);
    machine.set(Opt::AUD_SAMPLING_METHOD, i64(SamplingMethod::LINEAR));
    machine.set(Opt::AUD_ASR, 0);
    machine.set(Opt::AUD_FASTPATH, 0);
    machine.set(Opt::AUD_FILTER_TYPE, i64(FilterType::A500));
    machine.set(Opt::AUD_PAN0, 100);
    machine.set(Opt::AUD_PAN1, 300);
    machine.set(Opt::AUD_PAN2, 300);
    machine.set(Opt::AUD_PAN3, 100);
    machine.set(Opt::AUD_VOL0, 100);
    machine.set(Opt::AUD_VOL1, 100);
    machine.set(Opt::AUD_VOL2, 100);
    machine.set(Opt::AUD_VOL3, 100);
    machine.set(Opt::AUD_VOLL, 50);
    machine.set(Opt::AUD_VOLR, 50);

    wait_until(
        session,
        messages,
        [&messages] {
            return messages.configMessages.load(std::memory_order_relaxed) > 0;
        },
        std::chrono::seconds(2),
        "the vAmiga configuration barrier");

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
        {Opt::HOST_SAMPLE_RATE, SAMPLE_RATE_HZ},
        {Opt::AUD_BUFFER_SIZE, AUDIO_BUFFER_FRAMES},
        {Opt::AUD_SAMPLING_METHOD, i64(SamplingMethod::LINEAR)},
        {Opt::AUD_ASR, 0},
        {Opt::AUD_FASTPATH, 0},
        {Opt::AUD_FILTER_TYPE, i64(FilterType::A500)},
        {Opt::AUD_PAN0, 100},
        {Opt::AUD_PAN1, 300},
        {Opt::AUD_PAN2, 300},
        {Opt::AUD_PAN3, 100},
        {Opt::AUD_VOL0, 100},
        {Opt::AUD_VOL1, 100},
        {Opt::AUD_VOL2, 100},
        {Opt::AUD_VOL3, 100},
        {Opt::AUD_VOLL, 50},
        {Opt::AUD_VOLR, 50},
    };
    for (const auto &[option, value] : expected) {
        const auto actual = machine.get(option);
        if (actual != value) {
            throw std::runtime_error(
                std::string("vAmiga configuration verification failed for ") +
                OptEnum::key(option));
        }
    }
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

[[nodiscard]] std::vector<float> drain_audio(Session &session)
{
    if (!session.isSuspended()) {
        throw std::logic_error("audio extraction requires a suspended machine");
    }
    const auto config = session.machine.audioPort.getConfig();
    const auto before = session.machine.audioPort.getStats();
    const auto count = static_cast<isize>(
        std::llround(before.fillLevel * static_cast<double>(config.bufferSize)));
    if (count < 0 || count > config.bufferSize) {
        throw std::runtime_error("vAmiga reported an invalid audio fill level");
    }
    if (count == 0) {
        return {};
    }

    std::vector<float> samples(static_cast<std::size_t>(count) * 2);
    const auto copied = session.machine.audioPort.copyInterleaved(samples.data(), count);
    if (copied != count) {
        throw std::runtime_error("vAmiga audio-buffer count changed while suspended");
    }
    for (const auto sample : samples) {
        if (!std::isfinite(sample)) {
            throw std::runtime_error("vAmiga produced a non-finite audio sample");
        }
    }

    const auto after = session.machine.audioPort.getStats();
    if (after.bufferUnderflows != before.bufferUnderflows ||
        after.bufferOverflows != before.bufferOverflows) {
        throw std::runtime_error("audio extraction changed vAmiga buffer-fault counters");
    }
    return samples;
}

void append_samples(std::vector<float> &destination, std::vector<float> source)
{
    destination.insert(destination.end(), source.begin(), source.end());
}

void write_u16(std::ostream &stream, std::uint16_t value)
{
    const char bytes[] {
        static_cast<char>(value),
        static_cast<char>(value >> 8),
    };
    stream.write(bytes, 2);
}

void write_u32(std::ostream &stream, std::uint32_t value)
{
    const char bytes[] {
        static_cast<char>(value),
        static_cast<char>(value >> 8),
        static_cast<char>(value >> 16),
        static_cast<char>(value >> 24),
    };
    stream.write(bytes, 4);
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

void write_float_wav(
    const std::filesystem::path &path,
    const std::vector<float> &samples)
{
    static_assert(sizeof(float) == 4);
    static_assert(std::numeric_limits<float>::is_iec559);
    if (samples.size() % 2 != 0) {
        throw std::runtime_error("interleaved capture has an incomplete stereo frame");
    }
    const auto frames = samples.size() / 2;
    const auto dataBytes64 = samples.size() * sizeof(float);
    if (dataBytes64 > std::numeric_limits<std::uint32_t>::max() - 50) {
        throw std::runtime_error("capture is too large for RIFF/WAVE");
    }
    const auto dataBytes = static_cast<std::uint32_t>(dataBytes64);

    const auto temporary = temporary_path(path);
    std::ofstream stream(temporary, std::ios::binary | std::ios::trunc);
    if (!stream) {
        throw std::runtime_error("cannot create temporary WAV");
    }
    stream.write("RIFF", 4);
    write_u32(stream, 50 + dataBytes);
    stream.write("WAVE", 4);

    stream.write("fmt ", 4);
    write_u32(stream, 18);
    write_u16(stream, 3);
    write_u16(stream, 2);
    write_u32(stream, static_cast<std::uint32_t>(SAMPLE_RATE_HZ));
    write_u32(stream, static_cast<std::uint32_t>(SAMPLE_RATE_HZ * 8));
    write_u16(stream, 8);
    write_u16(stream, 32);
    write_u16(stream, 0);

    stream.write("fact", 4);
    write_u32(stream, 4);
    write_u32(stream, static_cast<std::uint32_t>(frames));

    stream.write("data", 4);
    write_u32(stream, dataBytes);
    for (const auto sample : samples) {
        write_u32(stream, std::bit_cast<std::uint32_t>(sample));
    }
    stream.close();
    if (!stream) {
        throw std::runtime_error("cannot finish temporary WAV");
    }
    replace_with_temporary(temporary, path);
}

[[nodiscard]] std::string json_escape(const std::string &value)
{
    std::ostringstream result;
    for (const unsigned char character : value) {
        switch (character) {
            case '"':
                result << "\\\"";
                break;
            case '\\':
                result << "\\\\";
                break;
            case '\b':
                result << "\\b";
                break;
            case '\f':
                result << "\\f";
                break;
            case '\n':
                result << "\\n";
                break;
            case '\r':
                result << "\\r";
                break;
            case '\t':
                result << "\\t";
                break;
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

void write_number_array(std::ostream &stream, const std::vector<i64> &values)
{
    stream << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index != 0) {
            stream << ", ";
        }
        stream << values[index];
    }
    stream << ']';
}

void write_size_array(std::ostream &stream, const std::vector<std::size_t> &values)
{
    stream << '[';
    for (std::size_t index = 0; index < values.size(); ++index) {
        if (index != 0) {
            stream << ", ";
        }
        stream << values[index];
    }
    stream << ']';
}

void write_result(
    const std::filesystem::path &path,
    const ExpectedCase &expected,
    const ReadyRecord &ready,
    i64 readyEmulatorFrame,
    const std::vector<i64> &emulatorFrames,
    const std::vector<i64> &guestFields,
    const std::vector<std::size_t> &fieldSampleFrames,
    std::size_t totalSampleFrames,
    const AudioPortMetrics &audioStats)
{
    const auto temporary = temporary_path(path);
    std::ofstream stream(temporary, std::ios::trunc);
    if (!stream) {
        throw std::runtime_error("cannot create temporary adapter result");
    }

    stream << "{\n";
    stream << "  \"schema_version\": \"1.0.0\",\n";
    stream << "  \"producer_version\": \"" << json_escape(VAmiga::version()) << "\",\n";
    stream << "  \"producer_build\": \"" << json_escape(VAmiga::build()) << "\",\n";
    stream << "  \"case_id\": \"" << json_escape(expected.id) << "\",\n";
    stream << "  \"ready_emulator_frame\": " << readyEmulatorFrame << ",\n";
    stream << "  \"ready_record\": {\n";
    stream << "    \"case_number\": " << ready.number << ",\n";
    stream << "    \"schema\": " << ready.schema << ",\n";
    stream << "    \"field_counter\": " << ready.fieldCounter << ",\n";
    stream << "    \"channel\": " << ready.channel << ",\n";
    stream << "    \"period_cck\": " << ready.period << ",\n";
    stream << "    \"volume\": " << ready.volume << ",\n";
    stream << "    \"sample_word\": \"0x" << std::hex << std::setw(4)
           << std::setfill('0') << ready.sampleWord << std::dec << "\",\n";
    stream << "    \"sample_words\": " << ready.sampleWords << ",\n";
    stream << "    \"dmacon\": \"0x" << std::hex << std::setw(4)
           << std::setfill('0') << ready.dmacon << std::dec << "\",\n";
    stream << "    \"sample_address\": \"0x" << std::hex << std::setw(8)
           << std::setfill('0') << ready.sampleAddress << std::dec << "\",\n";
    stream << "    \"identity\": \"" << json_escape(ready.identity) << "\"\n";
    stream << "  },\n";
    stream << "  \"captured_emulator_frames\": ";
    write_number_array(stream, emulatorFrames);
    stream << ",\n";
    stream << "  \"captured_guest_fields\": ";
    write_number_array(stream, guestFields);
    stream << ",\n";
    stream << "  \"field_sample_frames\": ";
    write_size_array(stream, fieldSampleFrames);
    stream << ",\n";
    stream << "  \"sample_frames\": " << totalSampleFrames << ",\n";
    stream << "  \"audio_statistics\": {\n";
    stream << "    \"buffer_underflows\": " << audioStats.bufferUnderflows << ",\n";
    stream << "    \"buffer_overflows\": " << audioStats.bufferOverflows << ",\n";
    stream << "    \"produced_samples\": " << audioStats.producedSamples << ",\n";
    stream << "    \"consumed_samples\": " << audioStats.consumedSamples << ",\n";
    stream << "    \"final_fill_level\": " << std::setprecision(17)
           << audioStats.fillLevel << "\n";
    stream << "  }\n";
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

[[nodiscard]] ExpectedCase parse_expected_case(char **argv)
{
    ExpectedCase result {
        .id = argv[6],
        .number = parse_u16(argv[7], "case number"),
        .channel = parse_u16(argv[8], "channel"),
        .period = parse_u16(argv[9], "period"),
        .volume = parse_u16(argv[10], "volume"),
        .sampleWord = parse_u16(argv[11], "sample word"),
        .sampleWords = parse_u16(argv[12], "sample word count"),
        .identity = argv[13],
    };
    if (result.channel > 3) {
        throw std::runtime_error("channel must be in 0..3");
    }
    return result;
}

} // namespace

int main(int argc, char **argv)
{
    if (argc != 14) {
        std::cerr
            << "usage: vamiga-paula-audio-capture ROM ADF WAV CONFIG RESULT "
               "CASE_ID CASE_NUMBER CHANNEL PERIOD VOLUME SAMPLE_WORD "
               "SAMPLE_WORDS SERIAL_ID\n";
        return 64;
    }

    try {
        const auto wavPath = std::filesystem::path(argv[3]);
        const auto configPath = std::filesystem::path(argv[4]);
        const auto resultPath = std::filesystem::path(argv[5]);
        for (const auto &path : {wavPath, configPath, resultPath}) {
            if (std::filesystem::exists(path)) {
                throw std::runtime_error("refusing to overwrite " + path.string());
            }
        }

        const auto expected = parse_expected_case(argv);
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
                validate_ready_record(session.machine, ready, expected);
                if (ready.fieldCounter >= 8) {
                    readyEmulatorFrame = emulatorFrame;
                    settled = true;
                }
            }
            static_cast<void>(drain_audio(session));
            if (settled) {
                break;
            }
        }
        if (!settled) {
            throw std::runtime_error("PAUD probe did not settle within 600 fields");
        }

        std::vector<float> captured;
        std::vector<i64> emulatorFrames;
        std::vector<i64> guestFields;
        std::vector<std::size_t> fieldSampleFrames;
        for (int field = 0; field < 3; ++field) {
            const auto emulatorFrame = step_one_field(session, messages);
            if (read_long(session.machine, READY_BASE) != READY_MAGIC) {
                throw std::runtime_error("PAUD magic disappeared during capture");
            }
            const auto current = read_ready_record(session.machine);
            validate_ready_record(session.machine, current, expected);
            const auto expectedField = ready.fieldCounter + static_cast<u32>(field) + 1;
            if (current.fieldCounter != expectedField) {
                throw std::runtime_error("PAUD field counter is not consecutive");
            }

            auto samples = drain_audio(session);
            if (samples.empty() || samples.size() % 2 != 0) {
                throw std::runtime_error("captured field has no complete stereo audio");
            }
            emulatorFrames.push_back(emulatorFrame);
            guestFields.push_back(current.fieldCounter);
            fieldSampleFrames.push_back(samples.size() / 2);
            append_samples(captured, std::move(samples));
        }

        fail_on_runtime_message(messages);
        const auto audioStats = session.machine.audioPort.getStats();
        if (audioStats.bufferUnderflows != 0 || audioStats.bufferOverflows != 0) {
            throw std::runtime_error("vAmiga audio statistics contain a buffer fault");
        }
        session.halt();

        write_float_wav(wavPath, captured);
        write_result(
            resultPath,
            expected,
            ready,
            readyEmulatorFrame,
            emulatorFrames,
            guestFields,
            fieldSampleFrames,
            captured.size() / 2,
            audioStats);

        std::cout << "captured case=" << expected.id
                  << " ready_field=" << ready.fieldCounter
                  << " sample_frames=" << captured.size() / 2 << '\n';
        return 0;
    } catch (const std::exception &error) {
        std::cerr << "error: " << error.what() << '\n';
        return 1;
    }
}
