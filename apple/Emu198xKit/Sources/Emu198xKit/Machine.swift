import Emu198xCore
import Foundation

/// A call the emulator refused or could not complete.
public struct EmulatorError: Error, CustomStringConvertible, Sendable, Equatable {
    public enum Kind: Sendable, Equatable {
        /// The emulator refused the call, for example because firmware is missing.
        case refused
        /// An argument was unusable.
        case invalidArgument
        /// The emulator panicked during the call. The machine now refuses work.
        case panicked
        /// The machine panicked during an earlier call.
        case poisoned
    }

    public let kind: Kind
    public let message: String
    public var description: String { message }
}

/// Turns a status into an error. The message is read straight away, on the
/// same thread, because the next call replaces it.
func check(_ status: Int32) throws {
    guard status != EMU198X_OK else { return }
    let message = emu198x_last_error().map { String(cString: $0) } ?? "no message"
    let kind: EmulatorError.Kind = switch status {
    case EMU198X_INVALID_ARGUMENT: .invalidArgument
    case EMU198X_PANICKED: .panicked
    case EMU198X_POISONED: .poisoned
    default: .refused
    }
    throw EmulatorError(kind: kind, message: message)
}

/// Copies a library-owned buffer and frees it.
func take(_ buffer: Emu198xBuffer) -> Data {
    defer { emu198x_buffer_free(buffer) }
    guard let bytes = buffer.data else { return Data() }
    return Data(bytes: bytes, count: buffer.len)
}

/// A picture the machine produced: RGBA, `width * height * 4` bytes.
public struct Frame: Sendable, Equatable {
    public let width: Int
    public let height: Int
    public let pixels: [UInt8]
}

/// One emulated machine.
///
/// Not thread-safe: exactly one owner calls it, which in an app is
/// ``EmulatorLoop``'s thread. Each method forwards to the matching function
/// in `emu198x.h`, so the emulation is the same code the browser player runs.
public final class Machine {
    private let handle: OpaquePointer
    private var audioScratch = [Float](repeating: 0, count: 16_384)

    /// Creates an unbooted machine. Supply firmware, then call ``boot()``.
    public init(family: String, variant: String) throws {
        var created: OpaquePointer?
        try check(emu198x_machine_create(family, variant, &created))
        guard let created else {
            throw EmulatorError(kind: .refused, message: "no machine was returned")
        }
        handle = created
    }

    deinit {
        emu198x_machine_free(handle)
    }

    /// The compiled families and variants, as UTF-8 JSON.
    public static func catalogue() throws -> Data {
        var buffer = Emu198xBuffer(data: nil, len: 0)
        try check(emu198x_catalogue(&buffer))
        return take(buffer)
    }

    public func supplyFirmware(id: String, data: Data) throws {
        try data.withUnsafeBytes { bytes in
            let base = bytes.bindMemory(to: UInt8.self).baseAddress
            try check(emu198x_machine_firmware(handle, id, base, bytes.count))
        }
    }

    public func boot() throws {
        try check(emu198x_machine_boot(handle))
    }

    /// Runs exactly one frame, ignoring the clock.
    public func step() throws {
        try check(emu198x_machine_step(handle))
    }

    /// Runs the frames that `milliseconds` of wall-clock time owes, carrying
    /// any remainder to the next call. Returns how many ran.
    @discardableResult
    public func advance(milliseconds: Double) throws -> Int {
        var frames: UInt32 = 0
        try check(emu198x_machine_advance(handle, milliseconds, &frames))
        return Int(frames)
    }

    public func frameMilliseconds() throws -> Double {
        var period = 0.0
        try check(emu198x_machine_frame_ms(handle, &period))
        return period
    }

    public func load(slot: String, data: Data) throws {
        try data.withUnsafeBytes { bytes in
            let base = bytes.bindMemory(to: UInt8.self).baseAddress
            try check(emu198x_machine_load(handle, slot, base, bytes.count))
        }
    }

    /// Loads the inserted tape the way a person would from power-on: reset,
    /// wait for the prompt, type `LOAD ""` and press play. Spectrum only.
    /// Returns the frames spent booting; the tape then loads as the machine
    /// runs.
    @discardableResult
    public func autoloadTape(maxBootFrames: Int = 500) throws -> Int {
        guard let limit = UInt32(exactly: maxBootFrames) else {
            throw EmulatorError(kind: .invalidArgument, message: "boot frame limit out of range")
        }
        var frames: UInt32 = 0
        try check(emu198x_machine_autoload_tape(handle, limit, &frames))
        return Int(frames)
    }

    public func setTape(slot: String, playing: Bool) throws {
        try check(emu198x_machine_transport(handle, slot, playing))
    }

    public func setKey(_ name: String, pressed: Bool) throws {
        try check(emu198x_machine_key(handle, name, pressed))
    }

    /// Presses or releases a control on a numbered input port: the number
    /// printed on the machine, or 0 for its primary joystick (Kempston on a
    /// Spectrum), per `knowledge/decisions/joystick-port-numbering.md`.
    public func setButton(_ name: String, port: UInt8, pressed: Bool) throws {
        try check(emu198x_machine_button(handle, port, name, pressed))
    }

    public func configureAudio(sampleRate: Int) throws {
        guard let rate = UInt32(exactly: sampleRate) else {
            throw EmulatorError(kind: .invalidArgument, message: "sample rate out of range")
        }
        try check(emu198x_machine_configure_audio(handle, rate))
    }

    /// Every queued sample, interleaved stereo.
    public func drainAudio() throws -> [Float] {
        var drained: [Float] = []
        while true {
            var written = 0
            try audioScratch.withUnsafeMutableBufferPointer { scratch in
                try check(emu198x_machine_audio(handle, scratch.baseAddress, scratch.count, &written))
            }
            drained.append(contentsOf: audioScratch[..<written])
            if written < audioScratch.count { return drained }
        }
    }

    /// Width of one picture pixel relative to its height, as the runtime
    /// states its display: above 1 for a PAL Spectrum, 1 for an LCD.
    public func pixelAspectRatio() throws -> Float {
        var aspect: Float = 0
        try check(emu198x_machine_pixel_aspect(handle, &aspect))
        return aspect
    }

    /// The latest picture. Before the first frame has run it has no pixels.
    public func frame() throws -> Frame {
        var width: UInt32 = 0
        var height: UInt32 = 0
        try check(emu198x_machine_frame_size(handle, &width, &height))
        var pixels = [UInt8](repeating: 0, count: Int(width) * Int(height) * 4)
        var written = 0
        try pixels.withUnsafeMutableBufferPointer { buffer in
            try check(emu198x_machine_pixels(handle, buffer.baseAddress, buffer.count, &written))
        }
        pixels.removeLast(pixels.count - written)
        return Frame(width: Int(width), height: Int(height), pixels: pixels)
    }

    public func saveState() throws -> Data {
        var buffer = Emu198xBuffer(data: nil, len: 0)
        try check(emu198x_machine_save_state(handle, &buffer))
        return take(buffer)
    }

    public func restoreState(_ state: Data) throws {
        try state.withUnsafeBytes { bytes in
            let base = bytes.bindMemory(to: UInt8.self).baseAddress
            try check(emu198x_machine_restore_state(handle, base, bytes.count))
        }
    }

    /// Browser key codes mapped to machine key names, as UTF-8 JSON.
    public func keymap() throws -> Data {
        var buffer = Emu198xBuffer(data: nil, len: 0)
        try check(emu198x_machine_keymap(handle, &buffer))
        return take(buffer)
    }
}
