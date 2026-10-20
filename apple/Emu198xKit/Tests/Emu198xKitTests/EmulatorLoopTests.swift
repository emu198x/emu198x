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

    @Test func theAudioRingDropsTheOldestAndCountsShortReads() {
        let ring = AudioRing(capacity: 4)
        ring.write([1, 2, 3, 4, 5, 6])
        var out = [Float](repeating: -1, count: 6)
        let copied = out.withUnsafeMutableBufferPointer { ring.read(into: $0) }
        #expect(copied == 4)
        #expect(out == [3, 4, 5, 6, 0, 0])
        #expect(ring.statistics == AudioRing.Statistics(written: 6, dropped: 2, underruns: 1))
    }
}

/// A value the loop's thread sets and the test reads.
final class OSAllocatedUnfairLockBox<Value: Sendable>: Sendable {
    private let lock: OSAllocatedUnfairLock<Value>
    init(_ value: Value) { lock = OSAllocatedUnfairLock(initialState: value) }
    func set(_ value: Value) { lock.withLock { $0 = value } }
    func get() -> Value { lock.withLock { $0 } }
}
