import Foundation
import Testing
@testable import Emu198xKit

@Suite struct MachineTests {
    @Test func matchesDirectRustExecution() throws {
        let expected = try rustBaseline()
        #expect(expected.count == 50)
        let machine = try spectrum()
        let produced = try expected.indices.map { _ in try checkpoint(after: machine) }
        #expect(produced == expected)
        #expect(Set(produced.map(\.hash)).count > 10, "the border stripes move")
        #expect(produced.allSatisfy { Double(bitPattern: UInt64($0.energy) ?? 0) > 0 }, "the beeper sounds")
    }

    @Test func aSavedStateResumesIdentically() throws {
        let machine = try spectrum()
        // Read the warm-up's audio too, or the first checkpoint would also
        // drain thirty frames of backlog that the resumed run never has.
        for _ in 0..<30 { _ = try checkpoint(after: machine) }
        let start = try machine.saveState()
        let first = try (0..<10).map { _ in try checkpoint(after: machine) }
        let afterFirst = try machine.saveState()
        try machine.restoreState(start)
        let second = try (0..<10).map { _ in try checkpoint(after: machine) }
        #expect(start != afterFirst)
        #expect(try machine.saveState() == afterFirst)
        #expect(first == second)
    }

    @Test func refusalsArriveAsErrors() throws {
        #expect {
            try Machine(family: "sinclair-zx-spectrum", variant: "no-such-model")
        } throws: { error in
            let error = error as? EmulatorError
            return error?.kind == .refused && error?.message.contains("unknown compiled variant") == true
        }

        let unbooted = try Machine(family: "sinclair-zx-spectrum", variant: "spectrum_48k")
        #expect(throws: EmulatorError(kind: .refused, message: "boot the machine first")) {
            try unbooted.step()
        }
        #expect {
            try unbooted.boot()
        } throws: { error in
            (error as? EmulatorError)?.message.contains("missing") == true
        }

        let machine = try spectrum()
        #expect {
            try machine.advance(milliseconds: .nan)
        } throws: { error in
            (error as? EmulatorError)?.kind == .invalidArgument
        }
        try machine.step()
    }

    @Test func advanceRunsTheFramesWallClockTimeOwes() throws {
        let machine = try spectrum()
        let period = try machine.frameMilliseconds()
        #expect(period > 19 && period < 21, "a PAL Spectrum frame is about 20 ms")
        #expect(try machine.advance(milliseconds: period * 3.5) == 3)
        #expect(try machine.advance(milliseconds: period * 0.6) == 1)
    }

    @Test func aPALSpectrumHasWidePixels() throws {
        let aspect = try spectrum().pixelAspectRatio()
        #expect(aspect > 1.04 && aspect < 1.07, "7 MHz pixels on a PAL line: \(aspect)")
    }

    @Test func autoloadWithoutATapeIsRefusedByTheRuntime() throws {
        #expect {
            try spectrum().autoloadTape()
        } throws: { error in
            let error = error as? EmulatorError
            return error?.kind == .refused && error?.message.contains("tape") == true
        }
    }

    @Test(.enabled(if: stagedSpectrumROM() != nil, "needs the 48K ROM (EMU198X_SPECTRUM_48K_ROM)"))
    func autoloadTypesLoadAndPlaysTheTape() throws {
        let machine = try spectrum(rom: try #require(stagedSpectrumROM()))
        try machine.load(slot: "tape-1", data: printSevenTape())
        #expect(try machine.autoloadTape() > 0, "it waited for the ROM to boot")
    }

    @Test func machinesTearDownRepeatedly() throws {
        for _ in 0..<100 {
            let machine = try spectrum()
            try machine.step()
        }
    }

    @Test func theCatalogueListsTheSpectrum() throws {
        let json = String(decoding: try Machine.catalogue(), as: UTF8.self)
        #expect(json.contains("\"id\":\"spectrum_48k\""))
    }

    @Test(.enabled(if: stagedSpectrumROM() != nil, "needs the 48K ROM (EMU198X_SPECTRUM_48K_ROM)"))
    func theRealROMBootsToTheCopyrightScreen() throws {
        let machine = try spectrum(rom: try #require(stagedSpectrumROM()))
        for _ in 0..<150 { try machine.step() }
        let frame = try machine.frame()
        #expect(frame.pixels.count == frame.width * frame.height * 4)
        var colours = Set<UInt32>()
        for offset in stride(from: 0, to: frame.pixels.count, by: 4) {
            colours.insert(frame.pixels[offset..<offset + 4].reduce(0) { $0 << 8 | UInt32($1) })
        }
        #expect(colours.count > 1, "text on paper")
    }
}
