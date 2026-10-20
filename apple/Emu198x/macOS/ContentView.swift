import AppKit
import SwiftUI

struct ContentView: View {
    @Bindable var session: SpectrumSession
    @State private var firmwareProblem: String?

    var body: some View {
        VStack(spacing: 0) {
            if let loop = session.loop {
                ScreenView(loop: loop, session: session)
                if session.showsStatistics {
                    Text(session.statistics)
                        .font(.caption.monospacedDigit())
                        .padding(4)
                        .frame(maxWidth: .infinity)
                        .background(.black)
                        .foregroundStyle(.white)
                }
            } else {
                firmwarePrompt
            }
        }
        .frame(minWidth: 352, minHeight: 296)
        .background(.black)
        .task { startFromDefaultLocation() }
        .alert(item: $session.alert) { alert in
            Alert(title: Text(alert.title), message: Text(alert.message))
        }
    }

    private var firmwarePrompt: some View {
        VStack(spacing: 12) {
            Text(firmwareProblem ?? "Looking for the Spectrum 48K ROM…")
                .multilineTextAlignment(.center)
            Button("Choose ROM…") {
                let panel = NSOpenPanel()
                if panel.runModal() == .OK, let url = panel.url { start(from: url) }
            }
        }
        .foregroundStyle(.white)
        .padding()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func startFromDefaultLocation() {
        guard session.loop == nil else { return }
        start(from: SpectrumFirmware.defaultLocation)
    }

    private func start(from url: URL) {
        do {
            let rom = try url.withSecurityScope { try SpectrumFirmware.load(from: url) }
            firmwareProblem = nil
            session.start(rom: rom)
        } catch {
            firmwareProblem = String(describing: error)
        }
    }
}
