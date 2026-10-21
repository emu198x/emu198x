import CryptoKit
import Foundation

/// The Spectrum models the apps can run, by the runtime's variant name.
enum SpectrumModel: String, Codable, CaseIterable {
    case spectrum48k = "spectrum_48k"
    case spectrum128k = "spectrum_128k"

    var name: String {
        switch self {
        case .spectrum48k: "48K"
        case .spectrum128k: "128K"
        }
    }

    /// The ROM images the model needs, each accepted only as the
    /// unmodified published image.
    var firmware: [SpectrumFirmware.Image] {
        switch self {
        case .spectrum48k:
            // SHA-1 as `test-rom-policy.md` records it.
            [SpectrumFirmware.Image(
                id: "sinclair-zx-spectrum-48k-rom", file: "48.rom",
                sha1: "5ea7c2b824672e914525d1d5c419d71b84a426a2")]
        case .spectrum128k:
            // SHA-1s as MAME's `spec128` driver records the English ROMs.
            [
                SpectrumFirmware.Image(
                    id: "sinclair-zx-spectrum-128k-rom-0", file: "128-0.rom",
                    sha1: "4f4b11ec22326280bdb96e3baf9db4b4cb1d02c5"),
                SpectrumFirmware.Image(
                    id: "sinclair-zx-spectrum-128k-rom-1", file: "128-1.rom",
                    sha1: "80080644289ed93d71a1103992a154cc9802b2fa"),
            ]
        }
    }
}

/// Sinclair ROMs, found where the other Emu198x hosts look for them and
/// accepted only as the unmodified published images.
enum SpectrumFirmware {
    struct Image {
        /// The runtime's firmware id.
        let id: String
        /// The file name the desktop hosts use under `~/.emu198x/roms`.
        let file: String
        let sha1: String
    }

    #if os(macOS)
    /// `~/.emu198x/roms/<romDir>/48.rom`, the path the desktop binaries use.
    static var defaultLocation: URL {
        FileManager.default.homeDirectoryForCurrentUser
            .appending(path: ".emu198x/roms/sinclair-zx-spectrum-48k/48.rom")
    }
    #endif

    enum Problem: Error, CustomStringConvertible {
        case unreadable(URL)
        case modified(URL)

        var description: String {
            switch self {
            case let .unreadable(url):
                "No Spectrum ROM at \(url.path(percentEncoded: false))."
            case let .modified(url):
                "\(url.lastPathComponent) is not the unmodified Sinclair ROM."
            }
        }
    }

    /// Reads the 48K ROM from `url`, wherever the person keeps it.
    static func load(from url: URL) throws(Problem) -> Data {
        try read(SpectrumModel.spectrum48k.firmware[0], at: url)
    }

    /// Reads every ROM `model` needs from `directory`, by the file names
    /// the desktop hosts use, keyed by firmware id.
    static func load(_ model: SpectrumModel, from directory: URL) throws(Problem) -> [String: Data] {
        var images: [String: Data] = [:]
        for image in model.firmware {
            images[image.id] = try read(image, at: directory.appending(path: image.file))
        }
        return images
    }

    private static func read(_ image: Image, at url: URL) throws(Problem) -> Data {
        guard let data = try? Data(contentsOf: url) else { throw .unreadable(url) }
        let digest = Insecure.SHA1.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard digest == image.sha1 else { throw .modified(url) }
        return data
    }
}
