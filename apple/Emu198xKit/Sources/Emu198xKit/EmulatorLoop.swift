import Foundation
import os

/// A request from the host to the running machine. The loop applies
/// commands between frames in the order they were sent, so a menu item, a
/// shortcut and a controller all reach the machine by the same path, as
/// `knowledge/decisions/native-menu-shell.md` requires of the desktop host.
public enum HostCommand: Sendable {
    case key(String, pressed: Bool)
    case button(String, port: UInt8, pressed: Bool)
    case mouseButton(String, pressed: Bool)
    case load(slot: String, data: Data)
    case tape(slot: String, playing: Bool)
    /// Types `LOAD ""` from power-on and plays the inserted tape (Spectrum).
    case autoloadTape
    case pause(Bool)
    case restoreState(Data)
    /// The reply runs on the loop's thread.
    case saveState(@Sendable (Result<Data, any Error>) -> Void)
    /// Asks the runtime a query path, such as `tape.playing`. The reply,
    /// JSON, runs on the loop's thread.
    case query(String, @Sendable (Result<Data, any Error>) -> Void)
}

/// The newest frame, handed from the loop to whatever draws it.
public final class FrameSlot: Sendable {
    private let latest = OSAllocatedUnfairLock<(frame: Frame?, sequence: Int)>(
        initialState: (nil, 0))

    func publish(_ frame: Frame) {
        latest.withLock { $0 = (frame, $0.sequence + 1) }
    }

    /// The newest frame if it is newer than `sequence`, with its own sequence.
    public func frame(after sequence: Int) -> (frame: Frame, sequence: Int)? {
        latest.withLock { slot in
            guard let frame = slot.frame, slot.sequence > sequence else { return nil }
            return (frame, slot.sequence)
        }
    }
}

/// Interleaved stereo samples from the loop to the audio output.
///
/// The loop writes a frame's worth of sound at a time, but the output
/// device reads smaller blocks on its own schedule. So the ring holds back
/// until `prebuffer` samples are queued, and plays silence meanwhile. If it
/// then runs dry, it counts one underrun and holds back again.
///
/// The machine and the device keep slightly different time: a Spectrum
/// frame's sound arrives rounded up to whole samples, and the two clocks
/// drift apart. Left alone, the ring would slowly fill until it dropped
/// samples, or empty until it ran dry. So while the queue strays more than
/// a quarter from where it began playing, the ring plays up to 0.5% faster
/// or slower, interpolating between samples, to bring the queue back.
/// Within that band it copies samples unchanged.
///
/// Bounded: when full, the oldest samples are dropped and counted, so both
/// kinds of glitch show up in ``statistics``.
public final class AudioRing: Sendable {
    public struct Statistics: Sendable, Equatable {
        public var written = 0
        public var dropped = 0
        public var underruns = 0
    }

    private struct State {
        var samples: [Float]
        var start = 0
        var count = 0
        var playing = false
        /// The queue length, averaged over about a fifth of a second so the
        /// rate does not follow each frame's arrival.
        var smoothedCount = 0.0
        /// The queue length when playing began, which the rate holds it to.
        /// A frame's sound arrives at once, so this can be most of a frame
        /// above `prebuffer`.
        var target = 0.0
        /// How far past `start` the next output frame falls, in frames.
        var phase = 0.0
        var statistics = Statistics()
    }

    private static let channels = 2
    private static let maximumRateChange = 0.005
    private static let band = 0.25
    private static let smoothing = 0.05

    private let state: OSAllocatedUnfairLock<State>
    private let prebuffer: Int

    public init(capacity: Int, prebuffer: Int = 0) {
        self.prebuffer = min(prebuffer, capacity)
        state = OSAllocatedUnfairLock(initialState: State(samples: Array(repeating: 0, count: capacity)))
    }

    func write(_ input: [Float]) {
        state.withLock { ring in
            let capacity = ring.samples.count
            for sample in input {
                if ring.count == capacity {
                    ring.start = (ring.start + 1) % capacity
                    ring.count -= 1
                    ring.statistics.dropped += 1
                }
                ring.samples[(ring.start + ring.count) % capacity] = sample
                ring.count += 1
            }
            ring.statistics.written += input.count
        }
    }

    /// Fills `out`, padding with silence when too few samples are queued or
    /// the ring is still filling. Returns how many real samples it wrote.
    @discardableResult
    public func read(into out: UnsafeMutableBufferPointer<Float>) -> Int {
        // Unchecked because the closure captures `out`, which is not
        // Sendable. The closure runs synchronously while the caller's buffer
        // is valid; copying through an array instead would allocate on the
        // audio thread.
        state.withLockUnchecked { [prebuffer] ring in
            let channels = Self.channels
            let capacity = ring.samples.count
            if !ring.playing && ring.count >= max(prebuffer, 1) {
                ring.playing = true
                ring.smoothedCount = Double(ring.count)
                ring.target = Double(ring.count)
                ring.phase = 0
            }
            let wanted = out.count / channels
            let queued = ring.count / channels
            var written = 0
            if ring.playing {
                ring.smoothedCount += (Double(ring.count) - ring.smoothedCount) * Self.smoothing
                let step = prebuffer > 0
                    ? Self.rate(queued: ring.smoothedCount, target: ring.target) : 1
                var position = ring.phase
                while written < wanted {
                    let index = Int(position)
                    let fraction = Float(position - Double(index))
                    let needed = fraction == 0 ? index : index + 1
                    if needed >= queued { break }
                    for channel in 0..<channels {
                        let at = ring.start + index * channels + channel
                        let here = ring.samples[at % capacity]
                        let next = fraction == 0 ? here : ring.samples[(at + channels) % capacity]
                        out[written * channels + channel] = here + (next - here) * fraction
                    }
                    written += 1
                    position += step
                }
                let consumed = min(Int(position), queued)
                ring.phase = position - Double(consumed)
                ring.start = (ring.start + consumed * channels) % capacity
                ring.count -= consumed * channels
                if written < wanted {
                    ring.statistics.underruns += 1
                    ring.playing = false
                }
            }
            for index in (written * channels)..<out.count {
                out[index] = 0
            }
            return written * channels
        }
    }

    /// Frames of queue to consume per output frame: 1 while the queue is
    /// within a quarter of `target` of it, then changing in proportion to
    /// how far beyond that it has strayed, up to half a percent at half of
    /// `target` away. Below the target there is only `target` of room, so
    /// the band leaves most of it for correcting.
    private static func rate(queued: Double, target: Double) -> Double {
        let error = (queued - target) / target
        let beyond = error > band ? error - band : error < -band ? error + band : 0
        let change = beyond / band * maximumRateChange
        return 1 + max(-maximumRateChange, min(maximumRateChange, change))
    }

    public var statistics: Statistics {
        state.withLock { $0.statistics }
    }

    /// Discards queued samples, so sound from before a restored state is
    /// not played after it.
    public func clear() {
        state.withLock { ring in
            ring.start = 0
            ring.count = 0
            ring.playing = false
            ring.phase = 0
        }
    }
}

/// Runs one machine in real time on a thread of its own.
///
/// The machine is created on that thread and never leaves it. The host
/// talks to it only through ``send(_:)``, and frames and sound come back
/// through ``frames`` and ``audio``. The loop's own clock decides how many
/// frames run, so a display's refresh rate never changes emulation speed.
///
/// The thread keeps the loop alive until ``stop()``.
public final class EmulatorLoop: Sendable {
    public let frames = FrameSlot()
    public let audio: AudioRing

    private struct Control {
        var commands: [HostCommand] = []
        var stopping = false
        var finished = false
        var failure: EmulatorError?
        var commandFailures: [EmulatorError] = []
        var framesRun = 0
        /// Wall-clock time the loop has measured while running, and the part
        /// of it the catch-up limit discarded after stalls.
        var clockMilliseconds = 0.0
        var skippedMilliseconds = 0.0
        /// Audio values handed to ``audio``, both channels counted.
        var samplesWritten = 0
    }

    private let control = OSAllocatedUnfairLock(initialState: Control())

    /// Longest stretch of wall-clock time one pass will try to catch up.
    /// After a stall the machine resumes rather than racing through the gap.
    private static let catchUpLimitMilliseconds = 250.0

    /// Starts the thread. `makeMachine` runs on it and returns a booted
    /// machine; if it throws, the loop finishes with that ``failure``.
    public init(sampleRate: Int, makeMachine: @escaping @Sendable () throws -> Machine) {
        // Half a second of stereo, played once about 60 ms is queued: three
        // 50 Hz frames, so a late frame does not empty it.
        audio = AudioRing(capacity: sampleRate, prebuffer: sampleRate / 8)
        let thread = Thread { [self] in
            run(sampleRate: sampleRate, makeMachine: makeMachine)
        }
        thread.name = "Emu198x emulation"
        thread.qualityOfService = .userInteractive
        thread.start()
    }

    public func send(_ command: HostCommand) {
        control.withLock { $0.commands.append(command) }
    }

    /// Asks the loop to finish after the current pass.
    public func stop() {
        control.withLock { $0.stopping = true }
    }

    public var isFinished: Bool { control.withLock { $0.finished } }

    /// Why the loop stopped by itself, if it did.
    public var failure: EmulatorError? { control.withLock { $0.failure } }

    public var framesRun: Int { control.withLock { $0.framesRun } }

    /// What the loop has run, read together: frames and audio values
    /// produced, the wall-clock time it measured while running, and the
    /// part of that it chose not to catch up after stalls. The measured time
    /// less the skipped time is what the machine was owed, so a test can
    /// check the pacing even when the thread is sometimes starved.
    struct Pacing: Sendable {
        var framesRun: Int
        var samplesWritten: Int
        var clockMilliseconds: Double
        var skippedMilliseconds: Double
    }

    var pacing: Pacing {
        control.withLock {
            Pacing(
                framesRun: $0.framesRun, samplesWritten: $0.samplesWritten,
                clockMilliseconds: $0.clockMilliseconds,
                skippedMilliseconds: $0.skippedMilliseconds)
        }
    }

    /// Commands the machine refused since the last call, such as a load of
    /// unreadable media. The loop keeps running after these.
    public func takeCommandFailures() -> [EmulatorError] {
        control.withLock { control in
            defer { control.commandFailures.removeAll() }
            return control.commandFailures
        }
    }

    private func run(sampleRate: Int, makeMachine: () throws -> Machine) {
        do {
            let machine = try makeMachine()
            try machine.configureAudio(sampleRate: sampleRate)
            let clock = ContinuousClock()
            var last = clock.now
            var paused = false
            while true {
                let (commands, stopping) = control.withLock { control in
                    defer { control.commands.removeAll() }
                    return (control.commands, control.stopping)
                }
                if stopping { break }
                for command in commands {
                    apply(command, to: machine, paused: &paused)
                }
                let now = clock.now
                let elapsed = (now - last).milliseconds
                last = now
                if !paused {
                    let owed = min(elapsed, Self.catchUpLimitMilliseconds)
                    let ran = try machine.advance(milliseconds: owed)
                    let samples: Int
                    if ran > 0 {
                        frames.publish(try machine.frame())
                        let sound = try machine.drainAudio()
                        audio.write(sound)
                        samples = sound.count
                    } else {
                        samples = 0
                    }
                    control.withLock { control in
                        control.framesRun += ran
                        control.samplesWritten += samples
                        control.clockMilliseconds += elapsed
                        control.skippedMilliseconds += elapsed - owed
                    }
                }
                Thread.sleep(forTimeInterval: 0.002)
            }
        } catch {
            let failure = error as? EmulatorError
                ?? EmulatorError(kind: .refused, message: String(describing: error))
            control.withLock { $0.failure = failure }
        }
        control.withLock { $0.finished = true }
    }

    private func apply(_ command: HostCommand, to machine: Machine, paused: inout Bool) {
        do {
            switch command {
            case let .key(name, pressed): try machine.setKey(name, pressed: pressed)
            case let .button(name, port, pressed):
                try machine.setButton(name, port: port, pressed: pressed)
            case let .mouseButton(name, pressed):
                try machine.setMouseButton(name, pressed: pressed)
            case let .load(slot, data): try machine.load(slot: slot, data: data)
            case let .tape(slot, playing): try machine.setTape(slot: slot, playing: playing)
            case .autoloadTape:
                // The boot frames run at once; their sound is stale.
                try machine.autoloadTape()
                _ = try machine.drainAudio()
                audio.clear()
            case let .pause(value): paused = value
            case let .restoreState(state):
                try machine.restoreState(state)
                _ = try machine.drainAudio()
                audio.clear()
            case let .saveState(reply): reply(Result { try machine.saveState() })
            case let .query(path, reply): reply(Result { try machine.query(path) })
            }
        } catch {
            let failure = error as? EmulatorError
                ?? EmulatorError(kind: .refused, message: String(describing: error))
            control.withLock { $0.commandFailures.append(failure) }
        }
    }
}

extension Duration {
    var milliseconds: Double {
        let (seconds, attoseconds) = components
        return Double(seconds) * 1_000 + Double(attoseconds) / 1e15
    }
}
