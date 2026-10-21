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
    /// The keys the game asks for, in order, to choose its joystick and
    /// start (K in Dizzy; 3 then 1 in Crystal Kingdom Dizzy), pressed for
    /// the player on their first move after the game loads.
    var selectKeys: [String] = []

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
    /// it changes: `{"stick": "kempston", "selectKeys": ["KeyK"]}`. A
    /// single `selectKey`, as the first version wrote, still reads.
    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let defaults = SpectrumControls()
        stick = try container.decodeIfPresent(Stick.self, forKey: .stick) ?? defaults.stick
        keys = try container.decodeIfPresent([Control: String].self, forKey: .keys) ?? defaults.keys
        let single = try decoder.container(keyedBy: LegacyKeys.self)
            .decodeIfPresent(String.self, forKey: .selectKey)
        selectKeys = try container.decodeIfPresent([String].self, forKey: .selectKeys)
            ?? single.map { [$0] } ?? []
    }

    private enum LegacyKeys: String, CodingKey {
        case selectKey
    }
}

/// What the player changed from a game's own controls. Only the changes
/// are kept, so a later fix to the game's own controls still reaches every
/// setting the player left alone.
struct SpectrumControlsOverrides: Codable, Equatable {
    var stick: SpectrumControls.Stick?
    /// Key codes by button; an empty code means the player chose none.
    var keys: [Control: String] = [:]
    var selectKeys: [String]?

    var isEmpty: Bool { stick == nil && keys.isEmpty && selectKeys == nil }

    /// The changes that turn `base` into `chosen`. A setting the player
    /// puts back to the game's own is no longer a change.
    init(base: SpectrumControls, chosen: SpectrumControls) {
        stick = chosen.stick == base.stick ? nil : chosen.stick
        for control in SpectrumControls.buttons where chosen.keys[control] != base.keys[control] {
            keys[control] = chosen.keys[control] ?? ""
        }
        selectKeys = chosen.selectKeys == base.selectKeys ? nil : chosen.selectKeys
    }

    func applied(to base: SpectrumControls) -> SpectrumControls {
        var controls = base
        if let stick { controls.stick = stick }
        for (control, code) in keys {
            controls.keys[control] = code.isEmpty ? nil : code
        }
        if let selectKeys { controls.selectKeys = selectKeys }
        return controls
    }
}

/// Each game's controls: its own, from its library entry or else its
/// `<name>.controls.json` beside it, with the player's changes kept in
/// `UserDefaults` under the game's key.
enum SpectrumControlsStore {
    private static func key(for game: String) -> String { "controlOverrides.\(game)" }

    /// The game's own controls: those its library entry gives, or else its
    /// `<name>.controls.json`, or else the defaults.
    static func base(for url: URL, defaults: SpectrumControls?) -> SpectrumControls {
        if let defaults { return defaults }
        let sidecar = url.deletingPathExtension().appendingPathExtension("controls.json")
        if let data = try? Data(contentsOf: sidecar),
           let controls = try? JSONDecoder().decode(SpectrumControls.self, from: data) {
            return controls
        }
        return SpectrumControls()
    }

    static func overrides(for game: String) -> SpectrumControlsOverrides? {
        guard let data = UserDefaults.standard.data(forKey: key(for: game)) else { return nil }
        return try? JSONDecoder().decode(SpectrumControlsOverrides.self, from: data)
    }

    /// Keeps the player's changes, or forgets them when there are none.
    static func save(_ overrides: SpectrumControlsOverrides, for game: String) {
        guard !overrides.isEmpty, let data = try? JSONEncoder().encode(overrides) else {
            UserDefaults.standard.removeObject(forKey: key(for: game))
            return
        }
        UserDefaults.standard.set(data, forKey: key(for: game))
    }
}
