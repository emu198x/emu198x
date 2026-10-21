import SwiftUI

/// The computer's keyboard on screen, for the keys a game asks for that a
/// remote or controller cannot press: Dizzy's K for Kempston and Space to
/// start, for example. The shift keys stay down for the next key, as
/// one-finger typing on the real machine needed. The game keeps running
/// behind it, so each key's effect shows straight away.
struct KeyPadView: View {
    let session: MachineSession
    /// Held shifts, by browser key code, until the next key.
    @State private var held: Set<String> = []

    private struct Key: Hashable {
        let label: String
        let code: String
    }

    private var isC64: Bool { session.model == .c64 }
    private var isAmiga: Bool { session.model?.isAmiga ?? false }

    private var rows: [[Key]] {
        let enter = Key(label: isC64 || isAmiga ? "RETURN" : "ENTER", code: "Enter")
        var rows: [[Key]] = [
            "1234567890".map { Key(label: String($0), code: "Digit\($0)") },
            "QWERTYUIOP".map { Key(label: String($0), code: "Key\($0)") },
            "ASDFGHJKL".map { Key(label: String($0), code: "Key\($0)") } + [enter],
            "ZXCVBNM".map { Key(label: String($0), code: "Key\($0)") }
                + [Key(label: "SPACE", code: "Space")],
        ]
        if isC64 {
            rows.append([Key(label: "RUN/STOP", code: "Escape")]
                + ["F1", "F3", "F5", "F7"].map { Key(label: $0, code: $0) })
        }
        if isAmiga {
            rows.append([Key(label: "ESC", code: "Escape")]
                + (1...10).map { Key(label: "F\($0)", code: "F\($0)") })
        }
        return rows
    }

    /// The shifts, held for the next key: Caps Shift and Symbol Shift on
    /// the Spectrum, SHIFT and the Commodore key on the C64, SHIFT and ALT
    /// on the Amiga.
    private var shifts: [Key] {
        if isC64 {
            [Key(label: "SHIFT", code: "ShiftLeft"), Key(label: "C=", code: "AltLeft")]
        } else if isAmiga {
            [Key(label: "SHIFT", code: "ShiftLeft"), Key(label: "ALT", code: "AltLeft")]
        } else {
            [Key(label: "CAPS SHIFT", code: "ShiftLeft"), Key(label: "SYMBOL SHIFT", code: "AltLeft")]
        }
    }

    var body: some View {
        VStack(spacing: 24) {
            ForEach(rows.indices, id: \.self) { row in
                HStack(spacing: 16) {
                    ForEach(rows[row], id: \.self) { key in
                        Button { press(key.code) } label: {
                            Text(key.label)
                                .font(.title3.monospaced())
                                .frame(minWidth: 60, minHeight: 50)
                        }
                    }
                }
            }
            HStack(spacing: 40) {
                ForEach(shifts, id: \.self) { shift in
                    Toggle(shift.label, isOn: holding(shift.code))
                }
            }
            .frame(maxWidth: 900)
        }
        .padding(40)
        .navigationTitle("Keyboard")
        // The machine runs while the keyboard is open: a key pressed on a
        // paused machine is released again before the ROM ever scans it.
        .onAppear { session.setPaused(false) }
        .onDisappear { session.setPaused(true) }
    }

    private func holding(_ code: String) -> Binding<Bool> {
        Binding {
            held.contains(code)
        } set: { on in
            if on { held.insert(code) } else { held.remove(code) }
        }
    }

    private func press(_ code: String) {
        let shifts = self.shifts.map(\.code).filter(held.contains)
        session.tap(codes: shifts + [code])
        held.removeAll()
    }
}
