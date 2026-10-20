import GameController

/// Maps the first connected game controller to the machine's joystick.
///
/// The d-pad and left stick are the four directions and A is fire.
/// Disconnecting a controller releases everything it held.
@MainActor
final class ControllerInput {
    private var observers: [NSObjectProtocol] = []
    private var held: Set<String> = []
    private let send: (String, Bool) -> Void

    /// `send` receives a joystick name (`up`, `down`, `left`, `right`,
    /// `fire`) and whether it is pressed.
    init(send: @escaping (String, Bool) -> Void) {
        self.send = send
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: .GCControllerDidConnect, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.attachAll() }
        })
        observers.append(center.addObserver(
            forName: .GCControllerDidDisconnect, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.releaseAll() }
        })
        attachAll()
    }

    private func attachAll() {
        GCController.controllers().forEach(attach)
    }

    private func attach(_ controller: GCController) {
        guard let pad = controller.extendedGamepad else {
            attachRemote(controller.microGamepad)
            return
        }
        pad.valueChangedHandler = { [weak self] pad, _ in
            let state = [
                "up": pad.dpad.up.isPressed || pad.leftThumbstick.yAxis.value > 0.5,
                "down": pad.dpad.down.isPressed || pad.leftThumbstick.yAxis.value < -0.5,
                "left": pad.dpad.left.isPressed || pad.leftThumbstick.xAxis.value < -0.5,
                "right": pad.dpad.right.isPressed || pad.leftThumbstick.xAxis.value > 0.5,
                "fire": pad.buttonA.isPressed,
            ]
            MainActor.assumeIsolated { self?.apply(state) }
        }
    }

    /// The Siri Remote: its touch surface or clickpad is the d-pad and a
    /// click is fire, for playing without a game controller.
    private func attachRemote(_ remote: GCMicroGamepad?) {
        guard let remote else { return }
        remote.reportsAbsoluteDpadValues = false
        remote.valueChangedHandler = { [weak self] remote, _ in
            let state = [
                "up": remote.dpad.yAxis.value > 0.5,
                "down": remote.dpad.yAxis.value < -0.5,
                "left": remote.dpad.xAxis.value < -0.5,
                "right": remote.dpad.xAxis.value > 0.5,
                "fire": remote.buttonA.isPressed,
            ]
            MainActor.assumeIsolated { self?.apply(state) }
        }
    }

    private func apply(_ state: [String: Bool]) {
        for (name, pressed) in state where pressed != held.contains(name) {
            if pressed { held.insert(name) } else { held.remove(name) }
            send(name, pressed)
        }
    }

    func releaseAll() {
        for name in held.sorted() { send(name, false) }
        held.removeAll()
    }
}
