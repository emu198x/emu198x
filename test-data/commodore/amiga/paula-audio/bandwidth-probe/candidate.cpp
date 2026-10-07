// Standalone research prototype. Not linked into the emulator.
// Independently generated, normalised Blackman-windowed sinc step response.
#include <array>
#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <stdexcept>
#include <string_view>
#include <vector>

constexpr int width = 96;
constexpr int phases = 256;
constexpr double pi = 3.14159265358979323846;
constexpr std::uint64_t host_hz = 48000;
using Stereo = std::array<double, 2>;

struct Kernel {
    std::array<std::array<double, width>, phases + 1> corrections{};
    Kernel() {
        std::array<double, width * phases + 1> integral{};
        double last = 0;
        for (int i = 0; i <= width * phases; ++i) {
            const double t = double(i) / phases;
            const double x = t - width / 2;
            const double w = 0.42 - 0.5 * std::cos(2 * pi * t / width)
                + 0.08 * std::cos(4 * pi * t / width);
            const double sinc = x == 0 ? 44000.0 / host_hz
                : std::sin(2 * pi * 22000.0 / host_hz * x) / (pi * x);
            const double y = w * sinc;
            if (i) integral[i] = integral[i - 1] + (last + y) / (2 * phases);
            last = y;
        }
        const double total = integral.back();
        for (double &v : integral) v /= total;
        for (int phase = 0; phase <= phases; ++phase)
            for (int sample = 0; sample < width; ++sample)
                corrections[phase][sample] = integral[sample * phases + phase] - 1;
    }
};

struct Candidate {
    const Kernel &kernel;
    std::array<Stereo, width> pending{};
    Stereo level{};
    int head = 0;
    explicit Candidate(const Kernel &k) : kernel(k) {}
    void edge(Stereo next, double until_output) {
        if (next == level) return;
        const Stereo delta{next[0] - level[0], next[1] - level[1]};
        level = next;
        const double position = until_output * phases;
        const int index = std::min(int(position), phases - 1);
        const double fraction = position - index;
        const auto &first = kernel.corrections[index];
        const auto &second = kernel.corrections[index + 1];
        // Two contiguous spans avoid modulo/index work inside the hot loop.
        for (int j = 0; j < width - head; ++j) {
            const double c = first[j] + fraction * (second[j] - first[j]);
            pending[head + j][0] += delta[0] * c;
            pending[head + j][1] += delta[1] * c;
        }
        for (int j = width - head; j < width; ++j) {
            const double c = first[j] + fraction * (second[j] - first[j]);
            pending[head + j - width][0] += delta[0] * c;
            pending[head + j - width][1] += delta[1] * c;
        }
    }
    Stereo emit() {
        Stereo result{level[0] + pending[head][0], level[1] + pending[head][1]};
        pending[head] = {};
        head = (head + 1) % width;
        return result;
    }
};

double amplitude(const std::vector<double> &samples, int hz) {
    double re = 0, im = 0, total = 0;
    for (std::size_t n = 0; n < samples.size(); ++n) {
        const double w = 0.5 - 0.5 * std::cos(2 * pi * n / samples.size());
        const double a = 2 * pi * hz * n / host_hz;
        re += samples[n] * w * std::cos(a);
        im += samples[n] * w * std::sin(a);
        total += w;
    }
    return 2 * std::hypot(re, im) / total;
}

void pulse_check(const Kernel &kernel) {
    for (double phase : {0.0, 0.1, 0.5, 0.99}) {
        Candidate candidate(kernel);
        candidate.edge({0.25, -0.125}, 1 - phase);
        candidate.edge({0, 0}, 1 - phase - 0.001);
        Stereo area{};
        for (int n = 0; n < width + 2; ++n) {
            const auto sample = candidate.emit();
            area[0] += sample[0]; area[1] += sample[1];
        }
        if (std::abs(area[0] - 0.00025) > 1e-8 ||
            std::abs(area[1] + 0.000125) > 1e-8)
            throw std::runtime_error("short-pulse area lost");
    }
}

int main(int argc, char **argv) {
    const bool bypass = argc == 2 && std::string_view(argv[1]) == "--bypass";
    if (argc > 1 && !bypass) throw std::runtime_error("unknown argument");
    const Kernel kernel;
    pulse_check(kernel);
    std::cout << std::setprecision(12);
    int count = 0, failures = 0;
    for (std::uint64_t hz : {7093790, 7159090}) {
        for (int frequency : {1000, 10000, 20000, 24000, 28000, 55000, 95000, 150000, 500000, 1000000, 3000000}) {
            for (double phase : {0.0, 0.37, 0.91}) {
                Candidate candidate(kernel);
                std::uint64_t clock = 0;
                int frames = 0;
                std::vector<double> samples;
                while (frames < 7200) {
                    const auto tick = (std::uint64_t(frames) * hz + clock) / host_hz;
                    const double v = std::sin(2 * pi * (double(tick * frequency % hz) / hz + phase));
                    candidate.edge({0, v}, double(hz - clock) / hz);
                    clock += host_hz;
                    if (clock >= hz) {
                        clock -= hz;
                        auto out = candidate.emit();
                        if (bypass) out = candidate.level;
                        if (out[0] != 0) throw std::runtime_error("channel leaked");
                        if (frames >= 2400) samples.push_back(out[1]);
                        ++frames;
                    }
                }
                const int fold = std::min(frequency % int(host_hz), int(host_hz) - frequency % int(host_hz));
                const double ratio = amplitude(samples, fold);
                const bool failed = frequency < 24000 ? std::abs(ratio - 1) > 0.001 : ratio > 0.001;
                failures += failed; ++count;
                std::cout << "candidate," << hz << ',' << frequency << ',' << phase << ','
                    << fold << ',' << ratio << ',' << failed << '\n';
            }
        }
    }
    // One emulated PAL second, stereo changes every source tick. This is
    // deliberately denser than Paula's output edges; no event queue can drop.
    for (int edge_ticks : {1,2,16,248}) {
        Candidate dense(kernel);
        std::uint64_t clock = 0;
        Stereo checksum{};
        const auto start = std::chrono::steady_clock::now();
        for (std::uint64_t tick = 0; tick < 7093790; ++tick) {
            const double v = (tick / edge_ticks) & 1 ? 0.25 : -0.25;
            dense.edge({v, -v}, double(7093790 - clock) / 7093790);
            clock += host_hz;
            if (clock >= 7093790) {
                clock -= 7093790;
                const auto sample = dense.emit();
                checksum[0] += sample[0]; checksum[1] += sample[1];
            }
        }
        const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
        std::cout << "stereo_benchmark,edge_ticks," << edge_ticks << ",seconds," << seconds
            << ",checksum," << checksum[0] << ',' << checksum[1] << '\n';
    }
    std::cout << "cases," << count << ",failures," << failures << ",kernel_bytes," << sizeof(Kernel)
        << ",state_bytes," << sizeof(Candidate) << ",delay_ms," << 1000.0 * width / 2 / host_hz << '\n';
    return failures == 0 && count == 66 ? 0 : 1;
}
