import Foundation

/// Turns host keys into machine keys using the runtime's own keymap.
///
/// The keymap sends some host keys to a chord: on the Spectrum the left
/// arrow is Caps Shift with 5. Machine keys are counted, so releasing one
/// host key does not release a Caps Shift another held key still needs.
struct KeyChords {
    /// Browser key code (`KeyA`, `ArrowLeft`) to machine key names.
    private let chords: [String: [String]]
    private var held: [String: Int] = [:]
    private var pressedCodes: Set<String> = []

    /// `json` is the output of `Machine.keymap()`.
    init(json: Data) throws {
        chords = try JSONDecoder().decode([String: [String]].self, from: json)
    }

    /// The machine keys whose state changes, and to what.
    mutating func set(code: String, pressed: Bool) -> [(key: String, pressed: Bool)] {
        guard let keys = chords[code], pressed != pressedCodes.contains(code) else { return [] }
        if pressed { pressedCodes.insert(code) } else { pressedCodes.remove(code) }
        var changes: [(key: String, pressed: Bool)] = []
        for key in keys {
            let count = held[key, default: 0] + (pressed ? 1 : -1)
            held[key] = count > 0 ? count : nil
            if pressed && count == 1 { changes.append((key, true)) }
            if !pressed && count == 0 { changes.append((key, false)) }
        }
        return changes
    }

    /// Releases everything, for focus loss.
    mutating func releaseAll() -> [(key: String, pressed: Bool)] {
        defer {
            held.removeAll()
            pressedCodes.removeAll()
        }
        return held.keys.sorted().map { ($0, false) }
    }
}
