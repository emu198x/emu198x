import SwiftUI

/// The Spectrum's keyboard on screen, for the keys a game asks for that a
/// remote or controller cannot press: Dizzy's K for Kempston and Space to
/// start, for example. Caps Shift and Symbol Shift stay down for the next
/// key, as one-finger typing on the real machine needed. The game keeps
/// running behind it, so each key's effect shows straight away.
struct KeyPadView: View {
    let session: MachineSession
    @State private var capsShift = false
    @State private var symbolShift = false

    private static let rows: [[(label: String, code: String)]] = [
        "1234567890".map { (String($0), "Digit\($0)") },
        "QWERTYUIOP".map { (String($0), "Key\($0)") },
        "ASDFGHJKL".map { (String($0), "Key\($0)") } + [("ENTER", "Enter")],
        "ZXCVBNM".map { (String($0), "Key\($0)") } + [("SPACE", "Space")],
    ]

    var body: some View {
        VStack(spacing: 24) {
            ForEach(Self.rows.indices, id: \.self) { row in
                HStack(spacing: 16) {
                    ForEach(Self.rows[row], id: \.code) { key in
                        Button { press(key.code) } label: {
                            Text(key.label)
                                .font(.title3.monospaced())
                                .frame(minWidth: 60, minHeight: 50)
                        }
                    }
                }
            }
            HStack(spacing: 40) {
                Toggle("CAPS SHIFT", isOn: $capsShift)
                Toggle("SYMBOL SHIFT", isOn: $symbolShift)
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

    private func press(_ code: String) {
        var codes: [String] = []
        if capsShift { codes.append("ShiftLeft") }
        if symbolShift { codes.append("AltLeft") }
        session.tap(codes: codes + [code])
        capsShift = false
        symbolShift = false
    }
}
