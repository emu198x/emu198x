import SwiftUI

@main
struct Emu198xSpectrumTVApp: App {
    @State private var session = SpectrumSession()

    var body: some Scene {
        WindowGroup {
            TVRootView(session: session)
        }
    }
}

/// The library until a program starts, then the machine full screen. The
/// remote's Menu button pauses and opens the pause menu.
struct TVRootView: View {
    @Bindable var session: SpectrumSession
    @State private var library = LocalLibrary()
    @State private var showsMenu = false
    /// The screen must hold focus, or the remote's Menu press reaches no
    /// view and tvOS leaves the app instead of pausing.
    @FocusState private var screenFocused: Bool

    var body: some View {
        Group {
            if let loop = session.loop {
                TVScreenView(loop: loop, pixelAspect: session.pixelAspect)
                    .ignoresSafeArea()
                    .focusable()
                    .focused($screenFocused)
                    .onAppear { screenFocused = true }
                    .onExitCommand { pauseAndShowMenu() }
                    // Full screen rather than a sheet: a tvOS sheet is a
                    // third of the screen wide, too narrow for the keyboard.
                    .fullScreenCover(isPresented: $showsMenu, onDismiss: resume) {
                        PauseMenu(session: session, showsMenu: $showsMenu)
                    }
            } else {
                LibraryView(library: library, start: start)
            }
        }
        .alert(item: $session.alert) { alert in
            Alert(title: Text(alert.title), message: Text(alert.message))
        }
    }

    private func start(_ program: LocalLibrary.Program) {
        guard let romURL = library.rom else { return }
        do {
            session.start(rom: try SpectrumFirmware.load(from: romURL))
            session.play(program.url)
        } catch {
            session.alert = SessionAlert(
                title: "The Spectrum could not start", message: String(describing: error))
        }
    }

    private func pauseAndShowMenu() {
        session.setPaused(true)
        showsMenu = true
    }

    private func resume() {
        if session.loop != nil { session.setPaused(false) }
        screenFocused = true
    }
}

struct LibraryView: View {
    let library: LocalLibrary
    let start: (LocalLibrary.Program) -> Void

    var body: some View {
        NavigationStack {
            Group {
                if library.rom == nil {
                    message("Put the Spectrum 48K ROM in apple/Emu198x/LocalContent as 48.rom, then build again.")
                } else if library.programs.isEmpty {
                    message("Put .tap, .tzx, .sna, .z80 or .state files in apple/Emu198x/LocalContent, then build again.")
                } else {
                    List(library.programs) { program in
                        Button { start(program) } label: {
                            LabeledContent(program.name, value: program.kind)
                        }
                    }
                }
            }
            .navigationTitle("Emu198x Spectrum")
        }
    }

    private func message(_ text: String) -> some View {
        Text(text)
            .multilineTextAlignment(.center)
            .padding(80)
    }
}

/// Paused: carry on, type keys the remote cannot, restart, or leave.
struct PauseMenu: View {
    let session: SpectrumSession
    @Binding var showsMenu: Bool

    var body: some View {
        NavigationStack {
            List {
                Button("Resume") { showsMenu = false }
                NavigationLink("Keyboard") { KeyPadView(session: session) }
                Button("Restart") {
                    session.restart()
                    showsMenu = false
                }
                Button("Back to Library", role: .destructive) {
                    session.stop()
                    showsMenu = false
                }
            }
            .navigationTitle("Paused")
            // Menu again resumes, as on any tvOS pause screen. In the
            // simulator Menu on the keyboard resumes too, rather than
            // stepping back to this menu.
            .onExitCommand { showsMenu = false }
        }
    }
}
