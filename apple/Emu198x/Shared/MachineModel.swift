import CryptoKit
import Emu198xKit
import Foundation

/// The machines the apps can run. The raw value is the name a library
/// entry uses for it; the Spectrum's are the runtime's variant names.
enum MachineModel: String, Codable, CaseIterable {
    case spectrum48k = "spectrum_48k"
    case spectrum128k = "spectrum_128k"
    /// NTSC: the Camerica cartridges are American.
    case nes
    /// PAL: Codemasters' Master System cartridges are European.
    case masterSystem = "master_system"
    case gameGear = "game_gear"
    /// PAL: Codemasters' C64 tapes are European.
    case c64
    /// An A500 with Kickstart 1.3 and the A501 trapdoor expansion, the
    /// 1 MB machine most Amiga games of the time ran on. Some need the
    /// extra 512K: Dizzy Panic and Kwik Snax crash on a bare 512K A500.
    case amiga500 = "amiga_500"
    /// An A1200 with Kickstart 3.1, for AGA games.
    case amiga1200 = "amiga_1200"

    /// The runtime family that builds it.
    var family: String {
        switch self {
        case .spectrum48k, .spectrum128k: "sinclair-zx-spectrum"
        case .nes: "nintendo-nes"
        case .masterSystem: "sega-master-system"
        case .gameGear: "sega-game-gear"
        case .c64: "commodore-c64"
        case .amiga500, .amiga1200: "commodore-amiga"
        }
    }

    /// The family's own name for this model.
    var variant: String {
        switch self {
        case .spectrum48k, .spectrum128k: rawValue
        case .nes: "nintendo-nes-ntsc"
        case .masterSystem: "sms-pal"
        case .gameGear: "game-gear"
        case .c64: "pal"
        case .amiga500: "a500-a501"
        case .amiga1200: "a1200"
        }
    }

    /// How a game's versions are told apart: "128K", "NES".
    var name: String {
        switch self {
        case .spectrum48k: "48K"
        case .spectrum128k: "128K"
        case .nes: "NES"
        case .masterSystem: "Master System"
        case .gameGear: "Game Gear"
        case .c64: "C64"
        case .amiga500: "Amiga"
        case .amiga1200: "Amiga 1200"
        }
    }

    /// The machine as a whole, for messages: "The Spectrum stopped".
    var system: String {
        switch self {
        case .spectrum48k, .spectrum128k: "Spectrum"
        case .amiga500, .amiga1200: "Amiga"
        default: name
        }
    }

    var isSpectrum: Bool { family == "sinclair-zx-spectrum" }
    var isAmiga: Bool { family == "commodore-amiga" }

    /// The slots a program's media go in, in order: a cartridge's slot, or
    /// an Amiga's drives DF0 to DF3. Media in an Amiga's drives before it
    /// boots fit those drives, so a two-disk game finds its second disk
    /// in DF1. Empty for a machine whose program is loaded after boot.
    var mediaSlots: [String] {
        switch self {
        case .nes, .masterSystem, .gameGear: ["cartridge-1"]
        case .amiga500, .amiga1200: ["floppy-0", "floppy-1", "floppy-2", "floppy-3"]
        case .spectrum48k, .spectrum128k, .c64: []
        }
    }

    /// The controls a game starts with when its entry names none.
    var defaultControls: GameControls {
        var controls = GameControls()
        if let stick = sticks.first { controls.stick = stick }
        // Most Amiga intros and menus wait for the left mouse button.
        if isAmiga { controls.keys = [.b: GameControls.leftMouse] }
        return controls
    }

    /// The joysticks a game on this machine can read, the first its
    /// default. A console has none: its controller is its ``pad``.
    var sticks: [GameControls.Stick] {
        switch self {
        case .spectrum48k, .spectrum128k: [.kempston, .sinclair, .cursor, .qaop]
        case .c64: [.port2, .port1]
        // Port 1 is the mouse's.
        case .amiga500, .amiga1200: [.port2]
        case .nes, .masterSystem, .gameGear: []
        }
    }

    /// Keys a controller button can press, as browser key codes with this
    /// machine's legends. The shifts are held like any other key.
    var keys: [(code: String, name: String)] {
        let digits = "1234567890".map { ("Digit\($0)", String($0)) }
        let letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ".map { ("Key\($0)", String($0)) }
        switch self {
        case .c64:
            return [("Space", "SPACE"), ("Enter", "RETURN"), ("ShiftLeft", "SHIFT"),
                    ("AltLeft", "C="), ("Escape", "RUN/STOP")]
                + ["F1", "F3", "F5", "F7"].map { ($0, $0) } + digits + letters
        case .amiga500, .amiga1200:
            return [(GameControls.leftMouse, "LEFT MOUSE BUTTON"),
                    (GameControls.rightMouse, "RIGHT MOUSE BUTTON"),
                    ("Space", "SPACE"), ("Enter", "RETURN"), ("Escape", "ESC"),
                    ("ShiftLeft", "SHIFT"), ("AltLeft", "ALT")]
                + (1...10).map { ("F\($0)", "F\($0)") } + digits + letters
        default:
            return [("Space", "SPACE"), ("Enter", "ENTER"), ("ShiftLeft", "CAPS SHIFT"),
                    ("AltLeft", "SYMBOL SHIFT")] + digits + letters
        }
    }

    /// The model a loose program runs on, by its file extension.
    init(fileExtension: String) {
        switch fileExtension.lowercased() {
        case "nes": self = .nes
        case "sms": self = .masterSystem
        case "gg": self = .gameGear
        case "adf": self = .amiga500
        default: self = .spectrum48k
        }
    }

    /// The ROM images the model needs, each accepted only as the
    /// unmodified published image. The consoles need none.
    var firmware: [Firmware.Image] {
        switch self {
        case .spectrum48k:
            // SHA-1 as `test-rom-policy.md` records it.
            [Firmware.Image(
                id: "sinclair-zx-spectrum-48k-rom", file: "48.rom",
                sha1: "5ea7c2b824672e914525d1d5c419d71b84a426a2")]
        case .spectrum128k:
            // SHA-1s as MAME's `spec128` driver records the English ROMs.
            [
                Firmware.Image(
                    id: "sinclair-zx-spectrum-128k-rom-0", file: "128-0.rom",
                    sha1: "4f4b11ec22326280bdb96e3baf9db4b4cb1d02c5"),
                Firmware.Image(
                    id: "sinclair-zx-spectrum-128k-rom-1", file: "128-1.rom",
                    sha1: "80080644289ed93d71a1103992a154cc9802b2fa"),
            ]
        case .c64:
            // SHA-1s as MAME's `c64` driver records 901226-01, 901227-03
            // and 901225-01.
            [
                Firmware.Image(
                    id: "commodore-c64-basic-rom", file: "basic.rom",
                    sha1: "79015323128650c742a3694c9429aa91f355905e"),
                Firmware.Image(
                    id: "commodore-c64-kernal-rom", file: "kernal.rom",
                    sha1: "1d503e56df85a62fee696e7618dc5b4e781df1bb"),
                Firmware.Image(
                    id: "commodore-c64-character-rom", file: "chargen.rom",
                    sha1: "adc7c31e18c7c7413d54802ef2f4193da14711aa"),
            ]
        case .amiga500:
            // SHA-1 as MAME's `amiga` driver records 315093-02, Kickstart 1.3.
            [Firmware.Image(
                id: "commodore-amiga-kickstart-rom", file: "kick13.rom",
                sha1: "891e9a547772fe0c6c19b610baf8bc4ea7fcb785")]
        case .amiga1200:
            // SHA-1 as WinUAE's ROM list records Kickstart 3.1 (A1200),
            // 40.68, the two chips as one image.
            [Firmware.Image(
                id: "commodore-amiga-kickstart-rom", file: "kick31a1200.rom",
                sha1: "e21545723fe8374e91342617604f1b3d703094f1")]
        case .nes, .masterSystem, .gameGear:
            []
        }
    }

    /// The console's own controller on a game controller, by position:
    /// the bottom face button is the console's left one (NES B, Sega 1)
    /// and the right face button its right one (NES A, Sega 2), with X
    /// and Y repeating them. Options is Start; on the NES the left
    /// shoulder is Select. Nil for a computer, whose controls are a game's
    /// own choice.
    var pad: [Control: ControlTarget]? {
        let buttons: (left: String, right: String)
        let start: ControlTarget
        var targets: [Control: ControlTarget] = [:]
        switch self {
        case .spectrum48k, .spectrum128k, .c64, .amiga500, .amiga1200:
            return nil
        case .nes:
            buttons = ("b", "a")
            start = .joystick("start", port: 1)
            targets[.leftShoulder] = .joystick("select", port: 1)
        case .masterSystem:
            buttons = ("button1", "button2")
            // The console's Pause button, which games use as one.
            start = .machineKey("pause")
        case .gameGear:
            buttons = ("button1", "button2")
            start = .machineKey("start")
        }
        for direction in [Control.up, .down, .left, .right] {
            targets[direction] = .joystick(direction.rawValue, port: 1)
        }
        targets[.a] = .joystick(buttons.left, port: 1)
        targets[.x] = .joystick(buttons.left, port: 1)
        targets[.b] = .joystick(buttons.right, port: 1)
        targets[.y] = .joystick(buttons.right, port: 1)
        targets[.options] = start
        return targets
    }
}

/// Machine ROMs, found where the other Emu198x hosts look for them and
/// accepted only as the unmodified published images.
enum Firmware {
    struct Image {
        /// The runtime's firmware id.
        let id: String
        /// The file name the desktop hosts use under `~/.emu198x/roms`.
        let file: String
        let sha1: String
    }

    #if os(macOS)
    /// `~/.emu198x/roms/<romDir>/48.rom`, the path the desktop binaries use.
    static var defaultLocation: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: ".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
    }
    #endif

    enum Problem: Error, CustomStringConvertible {
        case unreadable(URL)
        case modified(URL)

        var description: String {
            switch self {
            case let .unreadable(url):
                "No ROM at \(url.path(percentEncoded: false))."
            case let .modified(url):
                "\(url.lastPathComponent) is not the unmodified published ROM."
            }
        }
    }

    /// Reads the 48K ROM from `url`, wherever the person keeps it.
    static func load(from url: URL) throws(Problem) -> Data {
        try read(MachineModel.spectrum48k.firmware[0], at: url)
    }

    /// Reads every ROM `model` needs from `directory`, by the file names
    /// the desktop hosts use, keyed by firmware id.
    static func load(_ model: MachineModel, from directory: URL) throws(Problem) -> [String: Data] {
        var images: [String: Data] = [:]
        for image in model.firmware {
            images[image.id] = try read(image, at: directory.appending(path: image.file))
        }
        return images
    }

    private static func read(_ image: Image, at url: URL) throws(Problem) -> Data {
        guard let data = try? Data(contentsOf: url) else { throw .unreadable(url) }
        let digest = Insecure.SHA1.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard digest == image.sha1 else { throw .modified(url) }
        return data
    }
}
