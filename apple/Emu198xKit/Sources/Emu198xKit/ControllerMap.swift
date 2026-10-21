/// One physical control on a game controller. Its name is its key in
/// JSON, so a mapping reads `{"b": "Space"}`.
public enum Control: String, CaseIterable, Codable, CodingKeyRepresentable, Sendable {
    case up, down, left, right
    case a, b, x, y
    case leftShoulder, rightShoulder
    /// The Options or View button. The Menu button is the app's own: it
    /// pauses.
    case options
}

/// What a control drives on the machine.
public enum ControlTarget: Hashable, Codable, Sendable {
    /// A joystick input (`up`, `fire`) on a numbered input port, per
    /// `knowledge/decisions/joystick-port-numbering.md`.
    case joystick(String, port: UInt8)
    /// A host key by browser key code (`KeyQ`, `Space`), which reaches the
    /// machine through the runtime's keymap like a typed key.
    case key(String)
    /// A machine key by the runtime's own name, sent as it is: a console
    /// button with no host key, such as the Game Gear's `start`.
    case machineKey(String)
}

/// Routes controller controls to machine inputs.
///
/// Several controls can share a target, as when fire and a button are both
/// Space, so targets are counted: one is released only when nothing holds
/// it. Changing the map releases everything first, so no input stays held
/// under a mapping that no longer exists.
public struct ControllerMap: Sendable {
    public private(set) var targets: [Control: ControlTarget]
    private var heldControls: Set<Control> = []
    private var heldTargets: [ControlTarget: Int] = [:]

    public init(targets: [Control: ControlTarget]) {
        self.targets = targets
    }

    /// The machine inputs whose state changes, and to what.
    public mutating func set(_ control: Control, pressed: Bool) -> [(target: ControlTarget, pressed: Bool)] {
        guard let target = targets[control], pressed != heldControls.contains(control) else { return [] }
        if pressed { heldControls.insert(control) } else { heldControls.remove(control) }
        let count = heldTargets[target, default: 0] + (pressed ? 1 : -1)
        heldTargets[target] = count > 0 ? count : nil
        if pressed && count == 1 { return [(target, true)] }
        if !pressed && count == 0 { return [(target, false)] }
        return []
    }

    /// Releases everything held, for focus loss or a disconnected controller.
    public mutating func releaseAll() -> [(target: ControlTarget, pressed: Bool)] {
        defer {
            heldControls.removeAll()
            heldTargets.removeAll()
        }
        return heldTargets.keys.sorted { "\($0)" < "\($1)" }.map { ($0, false) }
    }

    /// Switches to `targets`, releasing what the old map held.
    public mutating func replace(with targets: [Control: ControlTarget]) -> [(target: ControlTarget, pressed: Bool)] {
        let released = releaseAll()
        self.targets = targets
        return released
    }
}
