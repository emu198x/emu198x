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

    private func start(_ version: LocalLibrary.Version) {
        guard let folder = library.firmware else { return }
        let firmware: [String: Data]
        do {
            firmware = try SpectrumFirmware.load(version.model, from: folder)
        } catch {
            let files = version.model.firmware.map(\.file).joined(separator: " and ")
            session.alert = SessionAlert(
                title: "The \(version.model.name) needs its ROM",
                message: "Put \(files) in apple/Emu198x/LocalContent, then build again. \(error)")
            return
        }
        session.start(version.model, firmware: firmware)
        session.play(version.url, game: version.key, controls: version.controls)
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
    let start: (LocalLibrary.Version) -> Void

    var body: some View {
        NavigationStack {
            Group {
                if library.games.isEmpty {
                    message("Put .tap, .tzx, .sna, .z80 or .state files and the Spectrum ROMs in apple/Emu198x/LocalContent, then build again.")
                } else {
                    ScrollView {
                        LazyVGrid(columns: [GridItem(.adaptive(minimum: 260), spacing: 48)], spacing: 64) {
                            ForEach(library.games) { game in
                                Button { start(game.versions[0]) } label: {
                                    CoverView(game: game)
                                }
                                .buttonStyle(.borderless)
                                // A long press offers every version.
                                .contextMenu {
                                    if game.versions.count > 1 {
                                        ForEach(game.versions) { version in
                                            Button(version.name) { start(version) }
                                        }
                                    }
                                }
                            }
                        }
                        .padding(.horizontal, 80)
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
                NavigationLink("Controls") { ControlsView(session: session) }
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

/// One game in the library: its box art, or its name on a plain card when
/// it has none, with its name and versions beneath. Selecting it plays the
/// first version.
struct CoverView: View {
    let game: LocalLibrary.Game

    var body: some View {
        VStack(spacing: 16) {
            Group {
                if let cover = game.cover, let image = UIImage(contentsOfFile: cover.path) {
                    Image(uiImage: image)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                } else {
                    Text(game.name)
                        .font(.headline)
                        .multilineTextAlignment(.center)
                        .padding()
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(.quaternary)
                }
            }
            // Cassette inlays are about 5:8.
            .frame(width: 260, height: 400)
            .clipped()
            VStack(spacing: 4) {
                Text(game.name)
                    .font(.caption)
                    .lineLimit(2)
                Text(game.versions.map(\.name).joined(separator: " · "))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .multilineTextAlignment(.center)
            .frame(width: 260)
        }
    }
}
