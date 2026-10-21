import Emu198xKit
import Foundation

/// How a controller plays one Spectrum game: which joystick the stick is,
/// which key each spare button presses, and the key that chooses the
/// joystick in the game's own menu.
struct SpectrumControls: Codable, Equatable {
    /// The joystick standards Spectrum games read.
    enum Stick: String, Codable, CaseIterable, Identifiable {
        /// Port $1F, through the runtime's primary joystick (port 0).
        case kempston
        /// Interface 2 port 1: keys 6 to 0.
        case sinclair
        /// Keys 5 to 8, with 0 as fire.
        case cursor
        /// Q, A, O and P, with Space as fire.
        case qaop

        var id: Self { self }

        var name: String {
            switch self {
            case .kempston: "Kempston"
            case .sinclair: "Sinclair"
            case .cursor: "Cursor"
            case .qaop: "QAOP"
            }
        }
    }

    /// The controls a person can give a key.
    static let buttons: [Control] = [.b, .x, .y, .leftShoulder, .rightShoulder]

    /// Keys a button can press, as browser key codes with their Spectrum
    /// legends. The shifts are held like any other key.
    static let keys: [(code: String, name: String)] =
        [("Space", "SPACE"), ("Enter", "ENTER"), ("ShiftLeft", "CAPS SHIFT"), ("AltLeft", "SYMBOL SHIFT")]
        + "1234567890".map { ("Digit\($0)", String($0)) }
        + "ABCDEFGHIJKLMNOPQRSTUVWXYZ".map { ("Key\($0)", String($0)) }

    var stick: Stick = .kempston
    /// Key codes by control. A button missing here does nothing.
    var keys: [Control: String] = [.b: "Space"]
    /// The key the game asks for to choose its joystick (K in Dizzy),
    /// pressed for the player on their first move after the game loads.
    var selectKey: String?

    var targets: [Control: ControlTarget] {
        var targets: [Control: ControlTarget] = [:]
        for (control, code) in keys where Self.buttons.contains(control) {
            targets[control] = .key(code)
        }
        let directions: [Control] = [.up, .down, .left, .right, .a]
        for (control, target) in zip(directions, stickTargets) {
            targets[control] = target
        }
        return targets
    }

    /// Up, down, left, right and fire, in that order.
    private var stickTargets: [ControlTarget] {
        switch stick {
        case .kempston: ["up", "down", "left", "right", "fire"].map { .joystick($0, port: 0) }
        case .sinclair: ["up", "down", "left", "right", "fire"].map { .joystick($0, port: 1) }
        case .cursor: ["Digit7", "Digit6", "Digit5", "Digit8", "Digit0"].map { .key($0) }
        case .qaop: ["KeyQ", "KeyA", "KeyO", "KeyP", "Space"].map { .key($0) }
        }
    }
}

extension SpectrumControls {
    /// Any field may be missing, so a hand-written file can name only what
    /// it changes: `{"stick": "kempston", "selectKey": "KeyK"}`.
    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = SpectrumControls()
        stick = try container.decodeIfPresent(Stick.self, forKey: .stick) ?? defaults.stick
        keys = try container.decodeIfPresent([Control: String].self, forKey: .keys) ?? defaults.keys
        selectKey = try container.decodeIfPresent(String.self, forKey: .selectKey)
    }
}

/// Each game's controls: what the player chose, kept in `UserDefaults` by
/// file name, or else the game's own `<name>.controls.json` beside it.
enum SpectrumControlsStore {
    private static func key(for game: String) -> String { "controls.\(game)" }

    static func load(for url: URL) -> SpectrumControls {
        let game = url.lastPathComponent
        let sidecar = url.deletingPathExtension().appendingPathExtension("controls.json")
        for data in [UserDefaults.standard.data(forKey: key(for: game)), try? Data(contentsOf: sidecar)] {
            if let data, let controls = try? JSONDecoder().decode(SpectrumControls.self, from: data) {
                return controls
            }
        }
        return SpectrumControls()
    }

    static func save(_ controls: SpectrumControls, for game: String) {
        guard let data = try? JSONEncoder().encode(controls) else { return }
        UserDefaults.standard.set(data, forKey: key(for: game))
    }
}
