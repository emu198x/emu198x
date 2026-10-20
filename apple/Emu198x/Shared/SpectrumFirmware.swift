import CryptoKit
import Foundation

/// The Sinclair 48K ROM, found where the other Emu198x hosts look for it
/// and accepted only as the unmodified published image.
enum SpectrumFirmware {
    static let id = "sinclair-zx-spectrum-48k-rom"

    /// The published ROM's SHA-1, as `test-rom-policy.md` records it.
    static let sha1 = "5ea7c2b824672e914525d1d5c419d71b84a426a2"

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
                "No Spectrum 48K ROM at \(url.path(percentEncoded: false))."
            case let .modified(url):
                "\(url.lastPathComponent) is not the unmodified Sinclair 48K ROM."
            }
        }
    }

    static func load(from url: URL) throws(Problem) -> Data {
        guard let data = try? Data(contentsOf: url) else { throw .unreadable(url) }
        let digest = Insecure.SHA1.hash(data: data).map { String(format: "%02x", $0) }.joined()
        guard digest == sha1 else { throw .modified(url) }
        return data
    }
}
