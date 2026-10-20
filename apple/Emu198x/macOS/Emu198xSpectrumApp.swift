import AppKit
import SwiftUI
import UniformTypeIdentifiers

@main
struct Emu198xSpectrumApp: App {
    @State private var session = SpectrumSession()

    var body: some Scene {
        Window("Emu198x Spectrum", id: "spectrum") {
            ContentView(session: session)
        }
        .commands { SpectrumCommands(session: session) }
    }
}

/// The menus. Each item calls the same session method its shortcut does.
struct SpectrumCommands: Commands {
    let session: SpectrumSession

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Open Tape or Snapshot…") {
                choose(["tap", "tzx", "sna", "z80"]) { session.open($0) }
            }
            .keyboardShortcut("o")
            .disabled(session.loop == nil)
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save State…") {
                let panel = NSSavePanel()
                panel.nameFieldStringValue = "Spectrum.state"
                if panel.runModal() == .OK, let url = panel.url { session.saveState(to: url) }
            }
            .keyboardShortcut("s")
            .disabled(session.loop == nil)
            Button("Load State…") {
                choose(["state"]) { session.loadState(from: $0) }
            }
            .keyboardShortcut("o", modifiers: [.command, .shift])
            .disabled(session.loop == nil)
        }
        CommandMenu("Machine") {
            Button(session.isPaused ? "Resume" : "Pause") { session.togglePause() }
                .keyboardShortcut("p")
                .disabled(session.loop == nil)
            Button("Restart") { session.restart() }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(session.loop == nil)
            Divider()
            Button(session.tapePlaying ? "Stop Tape" : "Play Tape") {
                session.setTapePlaying(!session.tapePlaying)
            }
            .keyboardShortcut("t")
            .disabled(session.loop == nil)
        }
        CommandGroup(after: .toolbar) {
            Toggle("Show Statistics", isOn: Binding(
                get: { session.showsStatistics }, set: { session.showsStatistics = $0 }))
                .keyboardShortcut("i")
        }
    }

    private func choose(_ extensions: [String], then open: (URL) -> Void) {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = extensions.compactMap { UTType(filenameExtension: $0) }
        if panel.runModal() == .OK, let url = panel.url { open(url) }
    }
}
