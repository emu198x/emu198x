import Emu198xKit
import GameController

/// Reports the connected game controllers' controls.
///
/// The d-pad and left stick are the four directions; A, B, X, Y and the
/// shoulder buttons are themselves. What each control does is the
/// session's ``ControllerMap``. Disconnecting a controller releases
/// everything it held.
@MainActor
final class ControllerInput {
    private var observers: [NSObjectProtocol] = []
    private var held: Set<Control> = []
    private let send: (Control, Bool) -> Void

    /// `send` receives a control and whether it is pressed.
    init(send: @escaping (Control, Bool) -> Void) {
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
            let state: [Control: Bool] = [
                .up: pad.dpad.up.isPressed || pad.leftThumbstick.yAxis.value > 0.5,
                .down: pad.dpad.down.isPressed || pad.leftThumbstick.yAxis.value < -0.5,
                .left: pad.dpad.left.isPressed || pad.leftThumbstick.xAxis.value < -0.5,
                .right: pad.dpad.right.isPressed || pad.leftThumbstick.xAxis.value > 0.5,
                .a: pad.buttonA.isPressed,
                .b: pad.buttonB.isPressed,
                .x: pad.buttonX.isPressed,
                .y: pad.buttonY.isPressed,
                .leftShoulder: pad.leftShoulder.isPressed,
                .rightShoulder: pad.rightShoulder.isPressed,
            ]
            MainActor.assumeIsolated { self?.apply(state) }
        }
    }

    /// The Siri Remote: its touch surface or clickpad is the d-pad, a click
    /// is A and Play/Pause is B, for playing without a game controller.
    private func attachRemote(_ remote: GCMicroGamepad?) {
        guard let remote else { return }
        remote.reportsAbsoluteDpadValues = false
        remote.valueChangedHandler = { [weak self] remote, _ in
            let state: [Control: Bool] = [
                .up: remote.dpad.yAxis.value > 0.5,
                .down: remote.dpad.yAxis.value < -0.5,
                .left: remote.dpad.xAxis.value < -0.5,
                .right: remote.dpad.xAxis.value > 0.5,
                .a: remote.buttonA.isPressed,
                .b: remote.buttonX.isPressed,
            ]
            MainActor.assumeIsolated { self?.apply(state) }
        }
    }

    private func apply(_ state: [Control: Bool]) {
        for (control, pressed) in state where pressed != held.contains(control) {
            if pressed { held.insert(control) } else { held.remove(control) }
            send(control, pressed)
        }
    }

    func releaseAll() {
        for control in Control.allCases where held.contains(control) { send(control, false) }
        held.removeAll()
    }
}
