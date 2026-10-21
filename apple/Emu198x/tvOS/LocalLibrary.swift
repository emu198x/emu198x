import Foundation

/// The programs and ROM built into this app from the git-ignored
/// `apple/Emu198x/LocalContent` folder (see its README). Local builds only:
/// nothing in that folder is committed or distributed.
struct LocalLibrary {
    struct Program: Identifiable, Hashable {
        let url: URL
        /// `<name>.jpg` or `<name>.png` beside the program: its box art.
        let cover: URL?
        var id: URL { url }
        var name: String { url.deletingPathExtension().lastPathComponent }
        var kind: String { url.pathExtension.uppercased() }
    }

    static let extensions = ["tap", "tzx", "sna", "z80", "state"]

    let programs: [Program]
    /// `48.rom`, the Spectrum 48K ROM, when one was put in LocalContent.
    let rom: URL?

    init(bundle: Bundle = .main) {
        programs = Self.extensions
            .flatMap { bundle.urls(forResourcesWithExtension: $0, subdirectory: nil) ?? [] }
            .map { url in
                let name = url.deletingPathExtension().lastPathComponent
                let cover = ["jpg", "png"].lazy
                    .compactMap { bundle.url(forResource: name, withExtension: $0) }
                    .first
                return Program(url: url, cover: cover)
            }
            .sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        rom = bundle.url(forResource: "48", withExtension: "rom")
    }
}
