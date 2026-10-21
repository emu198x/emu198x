import Foundation

/// The games and ROMs built into this app from the git-ignored
/// `apple/Emu198x/LocalContent` folder (see its README). Local builds only:
/// nothing in that folder is committed or distributed.
struct LocalLibrary {
    /// One game, with its box art and every version of it the folder holds.
    struct Game: Identifiable {
        let name: String
        /// `<name>.jpg` or `<name>.png`: its box art.
        let cover: URL?
        /// The first is the one selecting the game starts.
        let versions: [Version]
        var id: String { name }
    }

    /// One way to play a game: a program on a model, with the controls it
    /// starts with.
    struct Version: Identifiable {
        let name: String
        let url: URL
        let model: SpectrumModel
        let controls: SpectrumControls?
        /// The name its remembered controls are kept under.
        let key: String
        var id: String { key }
    }

    /// `<Game>.game.json`, listing a game's versions.
    private struct Manifest: Decodable {
        struct Entry: Decodable {
            let name: String
            let file: String
            let machine: SpectrumModel?
            let controls: SpectrumControls?
        }

        let versions: [Entry]
    }

    static let extensions = ["tap", "tzx", "sna", "z80", "state"]

    let games: [Game]
    /// Where the ROMs were put: the app's resources.
    let firmware: URL?

    init(bundle: Bundle = .main) {
        var games: [Game] = []
        var named: Set<String> = []
        for url in bundle.urls(forResourcesWithExtension: "json", subdirectory: nil) ?? []
        where url.lastPathComponent.hasSuffix(".game.json") {
            guard let data = try? Data(contentsOf: url),
                  let manifest = try? JSONDecoder().decode(Manifest.self, from: data)
            else { continue }
            let name = String(url.lastPathComponent.dropLast(".game.json".count))
            let versions = manifest.versions.compactMap { entry -> Version? in
                guard let file = bundle.url(forResource: entry.file, withExtension: nil) else { return nil }
                named.insert(entry.file)
                return Version(
                    name: entry.name, url: file, model: entry.machine ?? .spectrum48k,
                    controls: entry.controls, key: "\(name) (\(entry.name))")
            }
            guard !versions.isEmpty else { continue }
            games.append(Game(name: name, cover: Self.cover(named: name, in: bundle), versions: versions))
        }
        // Programs no game lists stand on their own, on the 48K.
        for url in Self.extensions.flatMap({ bundle.urls(forResourcesWithExtension: $0, subdirectory: nil) ?? [] })
        where !named.contains(url.lastPathComponent) {
            let name = url.deletingPathExtension().lastPathComponent
            let version = Version(
                name: url.pathExtension.uppercased(), url: url, model: .spectrum48k,
                controls: nil, key: url.lastPathComponent)
            games.append(Game(name: name, cover: Self.cover(named: name, in: bundle), versions: [version]))
        }
        self.games = games.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        firmware = bundle.resourceURL
    }

    private static func cover(named name: String, in bundle: Bundle) -> URL? {
        ["jpg", "png"].lazy.compactMap { bundle.url(forResource: name, withExtension: $0) }.first
    }
}
