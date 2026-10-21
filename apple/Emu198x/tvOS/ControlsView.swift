import Emu198xKit
import SwiftUI

/// How the controller plays the current game: which joystick the stick
/// is, and a key for each spare button. Changes apply at once and are
/// remembered for the game.
struct ControlsView: View {
    let session: MachineSession

    var body: some View {
        List {
            Section {
                // Segmented, the tvOS default here, which shows every
                // standard at once.
                Picker("Joystick", selection: stick) {
                    ForEach(session.model?.sticks ?? []) { stick in
                        Text(stick.name).tag(stick)
                    }
                }
            } header: {
                Text("Joystick")
            } footer: {
                Text("The d-pad, left stick and A, and the remote's touch surface and click.")
            }
            Section {
                Picker("Select key", selection: selectKey) {
                    Text("None").tag(String?.none)
                    ForEach(keys, id: \.code) { key in
                        Text(key.name).tag(String?.some(key.code))
                    }
                }
                .pickerStyle(.navigationLink)
            } footer: {
                Text(selectFooter)
            }
            Section("Buttons") {
                ForEach(GameControls.buttons, id: \.self) { control in
                    Picker(Self.label(control), selection: key(control)) {
                        Text("None").tag(String?.none)
                        ForEach(keys, id: \.code) { key in
                            Text(key.name).tag(String?.some(key.code))
                        }
                    }
                    // A list of its own: 40 keys do not fit in a row.
                    .pickerStyle(.navigationLink)
                }
            }
            Section {
                Button("Reset to the Game's Settings") { session.resetControls() }
                    .disabled(!session.hasChangedControls)
            } footer: {
                Text("Undoes your changes on this screen, so the game's own settings apply again, including any later fixes to them.")
            }
        }
        .navigationTitle(title)
    }

    private var title: String {
        guard let game = session.game else { return "Controls" }
        return "Controls for \((game as NSString).deletingPathExtension)"
    }

    /// The keys this machine's buttons can press, with its own legends.
    private var keys: [(code: String, name: String)] {
        session.model?.keys ?? []
    }

    private var stick: Binding<GameControls.Stick> {
        Binding {
            session.controls.stick
        } set: { stick in
            var controls = session.controls
            controls.stick = stick
            session.setControls(controls)
        }
    }

    /// The first select key. Choosing one here replaces a longer sequence
    /// from the game's controls file.
    private var selectKey: Binding<String?> {
        Binding {
            session.controls.selectKeys.first
        } set: { code in
            var controls = session.controls
            controls.selectKeys = code.map { [$0] } ?? []
            session.setControls(controls)
        }
    }

    private var selectFooter: String {
        let explanation = "The key the game asks for to choose a joystick, such as K in Dizzy. It is pressed for you on your first move once the game has loaded."
        let selectKeys = session.controls.selectKeys
        guard selectKeys.count > 1 else { return explanation }
        let names = selectKeys.map { code in
            keys.first { $0.code == code }?.name ?? code
        }
        return explanation + " This game's file presses " + names.joined(separator: ", ") + "."
    }

    private func key(_ control: Control) -> Binding<String?> {
        Binding {
            session.controls.keys[control]
        } set: { code in
            var controls = session.controls
            controls.keys[control] = code
            session.setControls(controls)
        }
    }

    private static func label(_ control: Control) -> String {
        switch control {
        case .b: "B (remote: Play/Pause)"
        case .x: "X"
        case .y: "Y"
        case .leftShoulder: "Left shoulder"
        case .rightShoulder: "Right shoulder"
        default: control.rawValue
        }
    }
}
