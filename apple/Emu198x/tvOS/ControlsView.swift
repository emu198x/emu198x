import Emu198xKit
import SwiftUI

/// How the controller plays the current game: which joystick the stick
/// is, and a key for each spare button. Changes apply at once and are
/// remembered for the game.
struct ControlsView: View {
    let session: SpectrumSession

    var body: some View {
        List {
            Section {
                // Segmented, the tvOS default here, which shows every
                // standard at once.
                Picker("Joystick", selection: stick) {
                    ForEach(SpectrumControls.Stick.allCases) { stick in
                        Text(stick.name).tag(stick)
                    }
                }
            } header: {
                Text("Joystick")
            } footer: {
                Text("The d-pad, left stick and A, and the remote's touch surface and click.")
            }
            Section("Buttons") {
                ForEach(SpectrumControls.buttons, id: \.self) { control in
                    Picker(Self.label(control), selection: key(control)) {
                        Text("None").tag(String?.none)
                        ForEach(SpectrumControls.keys, id: \.code) { key in
                            Text(key.name).tag(String?.some(key.code))
                        }
                    }
                    // A list of its own: 40 keys do not fit in a row.
                    .pickerStyle(.navigationLink)
                }
            }
        }
        .navigationTitle(title)
    }

    private var title: String {
        guard let game = session.game else { return "Controls" }
        return "Controls for \((game as NSString).deletingPathExtension)"
    }

    private var stick: Binding<SpectrumControls.Stick> {
        Binding {
            session.controls.stick
        } set: { stick in
            var controls = session.controls
            controls.stick = stick
            session.setControls(controls)
        }
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
