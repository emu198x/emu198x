import Emu198xKit
import Foundation
import Observation

/// Something the window should tell the person using it.
struct SessionAlert: Identifiable {
    let id = UUID()
    let title: String
    let message: String
}

/// The Spectrum one window shows: a running ``EmulatorLoop`` and what the
/// menus, screen and controls need to drive it.
///
/// Every action reaches the machine as a ``HostCommand``, applied by the
/// loop between frames, whether it came from a menu, a shortcut, the
/// keyboard or a controller.
@MainActor @Observable
final class SpectrumSession {
    nonisolated static let family = "sinclair-zx-spectrum"
    nonisolated static let variant = "spectrum_48k"
    nonisolated static let tapeSlot = "tape-1"

    private(set) var loop: EmulatorLoop?
    private(set) var isPaused = false
    private(set) var tapePlaying = false
    /// Width of one picture pixel relative to its height, from the runtime.
    private(set) var pixelAspect: Float = 1
    var alert: SessionAlert?
    var showsStatistics = false
    private(set) var statistics = ""
    /// The file name of the game started with ``play(_:)``, whose controls
    /// are remembered.
    private(set) var game: String?
    private(set) var controls = SpectrumControls()

    @ObservationIgnored private var rom: Data?
    @ObservationIgnored private var keyboard: KeyChords?
    @ObservationIgnored private let audio = AudioOutput()
    @ObservationIgnored private var controller: ControllerInput?
    @ObservationIgnored private var controllerMap = ControllerMap(targets: SpectrumControls().targets)
    /// The game's select key is still to be pressed: set when a game
    /// starts, and cleared by the first controller press once its tape has
    /// stopped.
    @ObservationIgnored private var selectPending = false
    /// Whether the tape is still playing, as the runtime last answered.
    @ObservationIgnored private var tapeBusy = false
    @ObservationIgnored private var monitor: Timer?
    @ObservationIgnored private var lastSample = (time: Date(), frames: 0)

    /// Boots a fresh 48K from `rom`, replacing any running machine.
    func start(rom: Data) {
        stop()
        do {
            let probe = try Self.makeMachine(rom: rom)
            keyboard = try KeyChords(json: probe.keymap())
            pixelAspect = try probe.pixelAspectRatio()
        } catch {
            report("The Spectrum could not start", error)
            return
        }
        self.rom = rom
        let loop = EmulatorLoop(sampleRate: AudioOutput.sampleRate) {
            try Self.makeMachine(rom: rom)
        }
        self.loop = loop
        isPaused = false
        tapePlaying = false
        audio.play(loop.audio)
        if controller == nil {
            controller = ControllerInput { [weak self] control, pressed in
                guard let self else { return }
                if pressed { pressSelectKeyIfDue() }
                route(controllerMap.set(control, pressed: pressed))
            }
        }
        lastSample = (Date(), 0)
        monitor = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.checkLoop() }
        }
    }

    func restart() {
        guard let rom else { return }
        start(rom: rom)
    }

    func stop() {
        monitor?.invalidate()
        monitor = nil
        audio.play(nil)
        loop?.stop()
        loop = nil
    }

    func togglePause() {
        setPaused(!isPaused)
    }

    func setPaused(_ paused: Bool) {
        guard paused != isPaused else { return }
        isPaused = paused
        loop?.send(.pause(paused))
    }

    /// Opens a snapshot (`.sna`, `.z80`) or inserts a tape (`.tap`, `.tzx`).
    /// A tape waits for the Spectrum's own LOAD command and Play Tape, as on
    /// the real machine and in the other Emu198x hosts.
    func open(_ url: URL) {
        let slot: String
        switch url.pathExtension.lowercased() {
        case "sna", "z80": slot = url.pathExtension.lowercased()
        case "tap", "tzx": slot = Self.tapeSlot
        default:
            alert = SessionAlert(
                title: "Unsupported file",
                message: "Open a .tap or .tzx tape, or a .sna or .z80 snapshot.")
            return
        }
        guard let data = read(url) else { return }
        loop?.send(.load(slot: slot, data: data))
        if slot == Self.tapeSlot { tapePlaying = false }
    }

    /// Starts a program with no typing: a tape is inserted and autoloaded
    /// (the runtime types `LOAD ""` and presses play), a snapshot is loaded,
    /// and a `.state` file saved by an Emu198x host is restored.
    func play(_ url: URL) {
        game = url.lastPathComponent
        apply(SpectrumControlsStore.load(for: url))
        selectPending = controls.selectKey != nil
        tapeBusy = false
        switch url.pathExtension.lowercased() {
        case "tap", "tzx":
            open(url)
            loop?.send(.autoloadTape)
            tapePlaying = true
            tapeBusy = true
        case "state":
            loadState(from: url)
        default:
            open(url)
        }
    }

    /// Changes how the controller plays this game, and remembers it. A new
    /// select key is pressed on the next controller press.
    func setControls(_ controls: SpectrumControls) {
        if controls.selectKey != self.controls.selectKey {
            selectPending = controls.selectKey != nil
        }
        apply(controls)
        if let game { SpectrumControlsStore.save(controls, for: game) }
    }

    private func apply(_ controls: SpectrumControls) {
        self.controls = controls
        route(controllerMap.replace(with: controls.targets))
    }

    /// Presses the game's select key before the player's first move, so a
    /// controller alone gets past a menu such as Dizzy's "K for Kempston".
    /// Held back while the tape plays, when the game is not yet listening.
    private func pressSelectKeyIfDue() {
        guard selectPending, !tapeBusy, let key = controls.selectKey else { return }
        selectPending = false
        tap(codes: [key])
    }

    /// Sends controller changes on: joystick inputs as buttons on their
    /// port, keys through the keymap like typed ones.
    private func route(_ changes: [(target: ControlTarget, pressed: Bool)]) {
        for change in changes {
            switch change.target {
            case let .joystick(name, port):
                loop?.send(.button(name, port: port, pressed: change.pressed))
            case let .key(code):
                key(code: code, pressed: change.pressed)
            }
        }
    }

    func setTapePlaying(_ playing: Bool) {
        tapePlaying = playing
        loop?.send(.tape(slot: Self.tapeSlot, playing: playing))
    }

    /// Writes the machine state to `url`. The file is the runtime's own
    /// state format, not the browser player's `.emu198x` save.
    func saveState(to url: URL) {
        loop?.send(.saveState { [weak self] result in
            let outcome = Result { try url.withSecurityScope { try result.get().write(to: url) } }
            Task { @MainActor in
                if case let .failure(error) = outcome {
                    self?.report("The state could not be saved", error)
                }
            }
        })
    }

    func loadState(from url: URL) {
        guard let data = read(url) else { return }
        loop?.send(.restoreState(data))
    }

    func key(code: String, pressed: Bool) {
        guard var keyboard else { return }
        let changes = keyboard.set(code: code, pressed: pressed)
        self.keyboard = keyboard
        for change in changes {
            loop?.send(.key(change.key, pressed: change.pressed))
        }
    }

    /// Presses keys together and releases them, holding them long enough for
    /// the ROM, which scans the keyboard once a frame, to see them. For
    /// on-screen keys, where a shift and a letter arrive as one tap.
    func tap(codes: [String]) {
        for code in codes { key(code: code, pressed: true) }
        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(100))
            for code in codes.reversed() { self?.key(code: code, pressed: false) }
        }
    }

    /// Releases every key and joystick direction, for when the window
    /// loses focus and would otherwise never see the key-up.
    func releaseAll() {
        guard var keyboard else { return }
        for change in keyboard.releaseAll() {
            loop?.send(.key(change.key, pressed: false))
        }
        self.keyboard = keyboard
        controller?.releaseAll()
    }

    /// Runs on the loop's thread as well as the main actor's.
    nonisolated private static func makeMachine(rom: Data) throws -> Machine {
        let machine = try Machine(family: family, variant: variant)
        try machine.supplyFirmware(id: SpectrumFirmware.id, data: rom)
        try machine.boot()
        return machine
    }

    private func read(_ url: URL) -> Data? {
        do {
            return try url.withSecurityScope { try Data(contentsOf: url) }
        } catch {
            report("\(url.lastPathComponent) could not be read", error)
            return nil
        }
    }

    private func checkLoop() {
        guard let loop else { return }
        if selectPending && tapeBusy {
            loop.send(.query("tape.playing") { [weak self] result in
                let playing = (try? result.get()) == Data("true".utf8)
                Task { @MainActor in self?.tapeBusy = playing }
            })
        }
        if let failure = loop.takeCommandFailures().first {
            report("The Spectrum refused that", failure)
        }
        if let failure = loop.failure {
            stop()
            report("The Spectrum stopped", failure)
            return
        }
        let now = Date()
        let frames = loop.framesRun
        let seconds = now.timeIntervalSince(lastSample.time)
        if seconds >= 1 {
            let rate = Double(frames - lastSample.frames) / seconds
            let audio = loop.audio.statistics
            statistics = String(
                format: "%.2f frames/s · %d underruns · %d samples dropped",
                rate, audio.underruns, audio.dropped)
            lastSample = (now, frames)
        }
    }

    private func report(_ title: String, _ error: any Error) {
        alert = SessionAlert(title: title, message: String(describing: error))
    }
}

extension URL {
    /// Runs `body` with access to a file the person chose, where the
    /// platform requires it.
    func withSecurityScope<T>(_ body: () throws -> T) rethrows -> T {
        let scoped = startAccessingSecurityScopedResource()
        defer { if scoped { stopAccessingSecurityScopedResource() } }
        return try body()
    }
}
