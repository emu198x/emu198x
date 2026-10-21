import Emu198xKit
import Foundation
import Observation

/// Something the window should tell the person using it.
struct SessionAlert: Identifiable {
    let id = UUID()
    let title: String
    let message: String
}

/// The machine one window shows: a running ``EmulatorLoop`` and what the
/// menus, screen and controls need to drive it.
///
/// Every action reaches the machine as a ``HostCommand``, applied by the
/// loop between frames, whether it came from a menu, a shortcut, the
/// keyboard or a controller.
@MainActor @Observable
final class MachineSession {
    nonisolated static let tapeSlot = "tape-1"
    nonisolated static let cartridgeSlot = "cartridge-1"

    private(set) var loop: EmulatorLoop?
    /// The machine running, or last run.
    private(set) var model: MachineModel?
    private(set) var isPaused = false
    private(set) var tapePlaying = false
    /// Width of one picture pixel relative to its height, from the runtime.
    private(set) var pixelAspect: Float = 1
    var alert: SessionAlert?
    var showsStatistics = false
    private(set) var statistics = ""
    /// The game started with ``play(_:game:controls:)``, by the name its
    /// controls are remembered under.
    private(set) var game: String?
    private(set) var controls = GameControls()
    /// Whether the player has changed this game's own controls.
    private(set) var hasChangedControls = false
    /// The game's own controls, which the player's changes sit on.
    @ObservationIgnored private var gameControls = GameControls()

    /// What the running machine was built from, for restart.
    @ObservationIgnored private var build: Build?
    @ObservationIgnored private var keyboard: KeyChords?
    @ObservationIgnored private let audio = AudioOutput()
    @ObservationIgnored private var controller: ControllerInput?
    @ObservationIgnored private var controllerMap = ControllerMap(targets: GameControls().targets)
    /// The game's select key is still to be pressed: set when a game
    /// starts, and cleared by the first controller press once its tape has
    /// stopped.
    @ObservationIgnored private var selectPending = false
    /// The select keys are being pressed, and the controller waits.
    @ObservationIgnored private var selecting = false
    /// Whether the tape is still loading, as the runtime last answered.
    @ObservationIgnored private var tapeBusy = false
    @ObservationIgnored private var monitor: Timer?
    @ObservationIgnored private var lastSample = (time: Date(), frames: 0)

    /// A model, its ROMs by firmware id, and the cartridge in its slot.
    private struct Build {
        let model: MachineModel
        let firmware: [String: Data]
        let cartridge: Data?
    }

    /// Boots a fresh 48K from `rom`, replacing any running machine.
    func start(rom: Data) {
        start(.spectrum48k, firmware: [MachineModel.spectrum48k.firmware[0].id: rom])
    }

    /// Boots a fresh `model` from `firmware`, its ROMs by firmware id, with
    /// `cartridge` in its slot, replacing any running machine. The
    /// cartridge is part of the machine, so restarting keeps it.
    func start(_ model: MachineModel, firmware: [String: Data], cartridge: Data? = nil) {
        stop()
        let build = Build(model: model, firmware: firmware, cartridge: cartridge)
        do {
            let probe = try Self.makeMachine(build)
            keyboard = try KeyChords(json: probe.keymap())
            pixelAspect = try probe.pixelAspectRatio()
        } catch {
            report("The \(model.system) could not start", error)
            return
        }
        self.build = build
        self.model = model
        let loop = EmulatorLoop(sampleRate: AudioOutput.sampleRate) {
            try Self.makeMachine(build)
        }
        self.loop = loop
        isPaused = false
        tapePlaying = false
        audio.play(loop.audio)
        if controller == nil {
            controller = ControllerInput { [weak self] control, pressed in
                guard let self else { return }
                if selecting || (pressed && pressSelectKeysIfDue()) { return }
                route(controllerMap.set(control, pressed: pressed))
            }
        }
        lastSample = (Date(), 0)
        monitor = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.checkLoop() }
        }
    }

    func restart() {
        guard let build else { return }
        start(build.model, firmware: build.firmware, cartridge: build.cartridge)
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

    /// Reads a cartridge to start a console with.
    func cartridge(at url: URL) -> Data? {
        read(url)
    }

    /// Starts a program with no typing: a tape is inserted and autoloaded
    /// (the runtime types `LOAD ""` and presses play), a snapshot is loaded,
    /// and a `.state` file saved by an Emu198x host is restored.
    ///
    /// `game` names the game for its remembered controls, the file name
    /// when not given; `controls` are the ones it starts with until the
    /// player changes them. A console's cartridge is already in it, from
    /// ``start(_:firmware:cartridge:)``, and its controller is its own.
    func play(_ url: URL, game: String? = nil, controls: GameControls? = nil) {
        let game = game ?? url.lastPathComponent
        self.game = game
        if let pad = model?.pad {
            hasChangedControls = false
            selectPending = false
            route(controllerMap.replace(with: pad))
            return
        }
        gameControls = GameControlsStore.base(for: url, defaults: controls)
        // A stick this machine has not got, as a Spectrum default on a C64,
        // becomes the machine's own first one.
        if let sticks = model?.sticks, !sticks.isEmpty, !sticks.contains(gameControls.stick) {
            gameControls.stick = sticks[0]
        }
        let changes = GameControlsStore.overrides(for: game)
        hasChangedControls = changes != nil
        apply(changes?.applied(to: gameControls) ?? gameControls)
        selectPending = !self.controls.selectKeys.isEmpty
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
        // Again after the load: a restored state carries its own Kempston
        // setting, which would unplug the one `apply` attached.
        attachKempstonIfChosen()
    }

    /// Changes how the controller plays this game, and remembers how that
    /// differs from the game's own controls. A new select key is pressed on
    /// the next controller press.
    func setControls(_ controls: GameControls) {
        if controls.selectKeys != self.controls.selectKeys {
            selectPending = !controls.selectKeys.isEmpty
        }
        apply(controls)
        let changes = GameControlsOverrides(base: gameControls, chosen: controls)
        hasChangedControls = !changes.isEmpty
        if let game { GameControlsStore.save(changes, for: game) }
    }

    /// Forgets the player's changes, so the game's own controls apply.
    func resetControls() {
        setControls(gameControls)
    }

    private func apply(_ controls: GameControls) {
        self.controls = controls
        route(controllerMap.replace(with: model?.pad ?? controls.targets))
        attachKempstonIfChosen()
    }

    /// Plugs the Kempston interface in when the stick is one: until a
    /// Kempston event arrives the runtime leaves it unattached, and games
    /// that look for one when they start (Fantasy World Dizzy) would not
    /// find it.
    private func attachKempstonIfChosen() {
        guard model?.isSpectrum ?? false, controls.stick == .kempston else { return }
        loop?.send(.button("fire", port: 0, pressed: false))
    }

    /// Presses the game's select keys before the player's first move, so a
    /// controller alone gets past a menu such as Dizzy's "K for Kempston".
    /// Held back while the tape plays, when the game is not yet listening.
    /// Returns whether it started, so the press that began it, and any
    /// until it ends, do not reach the game in the middle of its menus.
    private func pressSelectKeysIfDue() -> Bool {
        guard selectPending, !tapeBusy, !controls.selectKeys.isEmpty else { return false }
        selectPending = false
        selecting = true
        let keys = controls.selectKeys
        Task { @MainActor [weak self] in
            for key in keys {
                self?.key(code: key, pressed: true)
                try? await Task.sleep(for: .milliseconds(200))
                self?.key(code: key, pressed: false)
                // Time for the game to draw its next menu.
                try? await Task.sleep(for: .milliseconds(1_200))
            }
            self?.selecting = false
        }
        return true
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
            case let .machineKey(name):
                loop?.send(.key(name, pressed: change.pressed))
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
    nonisolated private static func makeMachine(_ build: Build) throws -> Machine {
        let machine = try Machine(family: build.model.family, variant: build.model.variant)
        for (id, data) in build.firmware.sorted(by: { $0.key < $1.key }) {
            try machine.supplyFirmware(id: id, data: data)
        }
        try machine.boot()
        if let cartridge = build.cartridge {
            try machine.load(slot: cartridgeSlot, data: cartridge)
        }
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
            checkTape(on: loop)
        }
        if let failure = loop.takeCommandFailures().first {
            report("The \(system) refused that", failure)
        }
        if let failure = loop.failure {
            stop()
            report("The \(system) stopped", failure)
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

    /// Asks whether the tape is still loading. The Spectrum's deck stops at
    /// the end of a load. A C64 loader stops the motor between blocks, for
    /// as long as 12 seconds after the KERNAL's FOUND, so the C64's tape
    /// is busy until it has played to its end.
    private func checkTape(on loop: EmulatorLoop) {
        guard model == .c64 else {
            loop.send(.query("tape.playing") { [weak self] result in
                let playing = (try? result.get()) == Data("true".utf8)
                Task { @MainActor in self?.tapeBusy = playing }
            })
            return
        }
        loop.send(.query("tape.pulse_index") { [weak self] index in
            loop.send(.query("tape.pulse_count") { count in
                let number = { (data: Data?) in data.flatMap { Int(String(decoding: $0, as: UTF8.self)) } }
                guard let index = number(try? index.get()), let count = number(try? count.get()) else { return }
                let loading = index + 1 < count
                Task { @MainActor in self?.tapeBusy = loading }
            })
        })
    }

    private var system: String { model?.system ?? "machine" }

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
