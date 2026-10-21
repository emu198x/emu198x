import Foundation
import os
import Testing
@testable import Emu198xKit

/// Polls until the loop's thread has exited.
func finish(_ loop: EmulatorLoop) async throws {
    loop.stop()
    for _ in 0..<200 where !loop.isFinished {
        try await Task.sleep(for: .milliseconds(10))
    }
    #expect(loop.isFinished, "the thread exits after stop()")
}

@Suite(.serialized) struct EmulatorLoopTests {
    @Test func runsInRealTime() async throws {
        let loop = EmulatorLoop(sampleRate: 48_000) { try spectrum() }
        try await Task.sleep(for: .seconds(1))
        let frames = loop.framesRun
        let written = loop.audio.statistics.written
        try await finish(loop)

        #expect(loop.failure == nil)
        // 1,000 ms of 19.97 ms frames is 50; scheduling allows a little slack.
        #expect((45...55).contains(frames), "ran \(frames) frames in a second")
        // One second of stereo at 48 kHz.
        #expect((86_000...106_000).contains(written), "produced \(written) samples")
        #expect(loop.frames.frame(after: 0) != nil)
    }

    @Test func pausingStopsTheMachine() async throws {
        let loop = EmulatorLoop(sampleRate: 48_000) { try spectrum() }
        try await Task.sleep(for: .milliseconds(200))
        loop.send(.pause(true))
        try await Task.sleep(for: .milliseconds(100))
        let paused = loop.framesRun
        try await Task.sleep(for: .milliseconds(300))
        #expect(loop.framesRun == paused)
        loop.send(.pause(false))
        try await Task.sleep(for: .milliseconds(300))
        #expect(loop.framesRun > paused)
        try await finish(loop)
    }

    @Test func savesAndFailedCommandsReachTheHost() async throws {
        let loop = EmulatorLoop(sampleRate: 48_000) { try spectrum() }
        let saved = OSAllocatedUnfairLockBox<Data?>(nil)
        loop.send(.saveState { result in saved.set(try? result.get()) })
        loop.send(.load(slot: "no-such-slot", data: Data([1, 2, 3])))
        try await Task.sleep(for: .milliseconds(200))
        #expect(saved.get()?.isEmpty == false)
        #expect(loop.takeCommandFailures().count == 1)
        #expect(loop.takeCommandFailures().isEmpty)
        #expect(loop.failure == nil, "a refused command does not stop the loop")
        try await finish(loop)
    }

    @Test func aMachineThatCannotStartEndsTheLoop() async throws {
        let loop = EmulatorLoop(sampleRate: 48_000) {
            try Machine(family: "sinclair-zx-spectrum", variant: "no-such-model")
        }
        try await Task.sleep(for: .milliseconds(100))
        #expect(loop.isFinished)
        #expect(loop.failure?.message.contains("unknown compiled variant") == true)
    }

    @Test func theAudioRingWaitsForItsPrebufferAndRefillsAfterRunningDry() {
        let ring = AudioRing(capacity: 16, prebuffer: 4)
        var out = [Float](repeating: -1, count: 2)
        func read() -> Int { out.withUnsafeMutableBufferPointer { ring.read(into: $0) } }

        ring.write([1, 2])
        #expect(read() == 0, "still filling")
        #expect(out == [0, 0])
        ring.write([3, 4])
        #expect(read() == 2)
        #expect(out == [1, 2])
        #expect(read() == 2)
        #expect(read() == 0, "ran dry")
        ring.write([5, 6])
        #expect(read() == 0, "refilling, so a single block does not play")
        #expect(ring.statistics.underruns == 1, "filling is not an underrun; running dry is")
    }

    @Test func theAudioRingDropsTheOldestAndCountsShortReads() {
        let ring = AudioRing(capacity: 4)
        ring.write([1, 2, 3, 4, 5, 6])
        var out = [Float](repeating: -1, count: 6)
        let copied = out.withUnsafeMutableBufferPointer { ring.read(into: $0) }
        #expect(copied == 4)
        #expect(out == [3, 4, 5, 6, 0, 0])
        #expect(ring.statistics == AudioRing.Statistics(written: 6, dropped: 2, underruns: 1))
    }

    /// Plays `seconds` of 50 Hz frames into the ring as a 48 kHz device
    /// reads 512-frame blocks. `drift` is how much faster the machine makes
    /// sound than the device plays it. Returns the ring and the samples read.
    private func play(seconds: Double, drift: Double, capture: Bool = false) -> (AudioRing, [Float]) {
        let rate = 48_000.0
        let ring = AudioRing(capacity: 48_000, prebuffer: 6_000)
        let framesPerMachineFrame = 960.0 * (1 + drift)
        var made = 0.0
        var sent = 0
        var value: Float = 0
        var block = [Float](repeating: 0, count: 1_024)
        var heard: [Float] = []
        let blocks = Int(rate * seconds / 512)
        for index in 0..<blocks {
            // Machine frames due by the time this block is read.
            let now = Double(index) * 512 / rate
            while Double(sent) / 50 <= now {
                made += framesPerMachineFrame
                let frames = Int(made) - Int(made - framesPerMachineFrame)
                var input: [Float] = []
                for _ in 0..<frames {
                    value += 1
                    input.append(value)
                    input.append(-value)
                }
                ring.write(input)
                sent += 1
            }
            _ = block.withUnsafeMutableBufferPointer { ring.read(into: $0) }
            if capture { heard.append(contentsOf: block) }
        }
        return (ring, heard)
    }

    /// A Spectrum makes 959 samples a channel for each 50.08 Hz frame, 0.056%
    /// more than a 48 kHz device plays, so an uncorrected half-second ring
    /// fills in about 13 minutes. These drifts are five times that, and 200
    /// seconds is long enough for them to overflow or run dry without
    /// correction.
    @Test(arguments: [0.003, -0.003])
    func theAudioRingAbsorbsDriftBetweenTheMachineAndTheDevice(drift: Double) {
        let (ring, _) = play(seconds: 200, drift: drift)
        let statistics = ring.statistics
        #expect(statistics.dropped == 0)
        #expect(statistics.underruns == 0)
    }

    @Test func aMatchedMachinePlaysEverySampleUnchanged() {
        let (ring, heard) = play(seconds: 10, drift: 0, capture: true)
        #expect(ring.statistics.dropped == 0)
        #expect(ring.statistics.underruns == 0)
        // Ignore the silence while the prebuffer fills.
        let played = Array(heard.drop { $0 == 0 })
        let expected = (0..<(played.count / 2)).flatMap { frame -> [Float] in
            let value = Float(frame + 1)
            return [value, -value]
        }
        #expect(played == expected, "no sample was interpolated, repeated or skipped")
    }
}

/// A value the loop's thread sets and the test reads.
final class OSAllocatedUnfairLockBox<Value: Sendable>: Sendable {
    private let lock: OSAllocatedUnfairLock<Value>
    init(_ value: Value) { lock = OSAllocatedUnfairLock(initialState: value) }
    func set(_ value: Value) { lock.withLock { $0 = value } }
    func get() -> Value { lock.withLock { $0 } }
}
