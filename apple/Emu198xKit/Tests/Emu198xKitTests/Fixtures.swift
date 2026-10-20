import Foundation
@testable import Emu198xKit

/// The synthetic beeper program from
/// crates/emu198x-apple/tests/common/beeper.rs, which explains it. If the
/// two copies drift apart, the baseline comparison fails.
func beeperROM() -> Data {
    var rom = Data(count: 16 * 1024)
    let program: [UInt8] = [0x3E, 0x00, 0xD3, 0xFE, 0xEE, 0x17, 0x06, 0xFF, 0x10, 0xFE, 0x18, 0xF6]
    rom.replaceSubrange(0..<program.count, with: program)
    return rom
}

let spectrumFirmwareID = "sinclair-zx-spectrum-48k-rom"

/// A booted Spectrum 48K running `rom`, with sound at 48 kHz.
func spectrum(rom: Data = beeperROM()) throws -> Machine {
    let machine = try Machine(family: "sinclair-zx-spectrum", variant: "spectrum_48k")
    try machine.supplyFirmware(id: spectrumFirmwareID, data: rom)
    try machine.boot()
    try machine.configureAudio(sampleRate: 48_000)
    return machine
}

/// One frame's output, in the form examples/baseline.rs prints.
struct Checkpoint: Decodable, Equatable {
    let hash: String
    let samples: Int
    let energy: String
}

/// FNV-1a, as the Rust side computes it.
func fnv1a(_ bytes: [UInt8]) -> UInt64 {
    bytes.reduce(0xcbf2_9ce4_8422_2325) { ($0 ^ UInt64($1)) &* 0x0000_0100_0000_01b3 }
}

func checkpoint(after machine: Machine) throws -> Checkpoint {
    try machine.step()
    let audio = try machine.drainAudio()
    let energy = audio.reduce(0.0) { $0 + Double(abs($1)) }
    return Checkpoint(
        hash: String(fnv1a(try machine.frame().pixels)),
        samples: audio.count,
        energy: String(energy.bitPattern))
}

/// apple/build/baseline.json, written by scripts/build-apple-xcframework.sh.
func rustBaseline() throws -> [Checkpoint] {
    let apple = URL(filePath: #filePath)
        .deletingLastPathComponent()  // Emu198xKitTests
        .deletingLastPathComponent()  // Tests
        .deletingLastPathComponent()  // Emu198xKit
        .deletingLastPathComponent()  // apple
    let data = try Data(contentsOf: apple.appending(path: "build/baseline.json"))
    return try JSONDecoder().decode([Checkpoint].self, from: data)
}

/// The real 48K ROM, where one is staged as the Rust tests expect it.
func stagedSpectrumROM() -> Data? {
    let environment = ProcessInfo.processInfo.environment
    let path = environment["EMU198X_SPECTRUM_48K_ROM"]
        ?? environment["HOME"].map { "\($0)/.emu198x/roms/sinclair-zx-spectrum-48k/48.rom" }
    return path.flatMap { FileManager.default.contents(atPath: $0) }
}

/// A two-block tape holding `10 PRINT 7`, as in the runtime's own autoload
/// test (`autoload_running_machine.rs`).
func printSevenTape() -> Data {
    func block(_ flag: UInt8, _ payload: [UInt8]) -> [UInt8] {
        let length = UInt16(payload.count + 2)
        let checksum = payload.reduce(flag, ^)
        return [UInt8(length & 0xFF), UInt8(length >> 8), flag] + payload + [checksum]
    }
    let program: [UInt8] = [0x00, 0x0A, 0x09, 0x00, 0xF5, 0x37, 0x0E, 0x00, 0x00, 0x07, 0x00, 0x00, 0x0D]
    let length = UInt16(program.count)
    let header: [UInt8] = [0x00] + Array("probe     ".utf8)
        + [UInt8(length & 0xFF), UInt8(length >> 8), 10, 0, UInt8(length & 0xFF), UInt8(length >> 8)]
    return Data(block(0x00, header) + block(0xFF, program))
}
