import Foundation

/// The command catalogue every host builds its menus, shortcuts and pause
/// menu from: `emu198x_shell::commands`, read through `emu198x_commands`.
/// A command means the same thing on every host, per
/// `knowledge/decisions/host-ui-model.md` § 3.
public enum CommandCatalogue {
    /// Every command, in menu order.
    public static func load() throws -> [Command] {
        try JSONDecoder().decode([Command].self, from: Machine.commands())
    }

    /// One entry in the catalogue.
    public struct Command: Decodable, Equatable, Sendable {
        /// Stable id, lower case and dotted, such as `media.tape.play`.
        public let id: String
        /// The menu label. A parameterised command's label holds `{}` where
        /// the parameter's name goes; use ``label(for:)``.
        public let label: String
        /// The menu it sits in: `app`, `file`, `machine`, `tape`, `state`
        /// or `view`.
        public let group: String
        public let shortcut: Shortcut?
        /// What the command needs before a host offers it, such as
        /// `running` or `tape-slot`.
        public let needs: String
        /// The value a parameterised command takes, such as `media-slot`.
        public let param: String?
        /// The MCP tool that does the same thing, if there is one.
        public let mcpVerb: String?

        enum CodingKeys: String, CodingKey {
            case id, label, group, shortcut, needs, param
            case mcpVerb = "mcp_verb"
        }

        /// The label with `name` in place of `{}`. A label without `{}` is
        /// returned unchanged.
        public func label(for name: String) -> String {
            guard let slot = label.range(of: "{}") else { return label }
            return label.replacingCharacters(in: slot, with: name)
        }
    }

    /// A keyboard shortcut. `host` is the host modifier, Command on the Mac.
    public struct Shortcut: Decodable, Equatable, Sendable {
        public enum Key: Equatable, Sendable {
            /// A letter key, `A` to `Z`.
            case letter(Character)
            /// A function key, 1 to 12.
            case function(Int)
        }

        public let key: Key
        public let host: Bool
        public let shift: Bool

        public init(key: Key, host: Bool, shift: Bool) {
            self.key = key
            self.host = host
            self.shift = shift
        }

        enum CodingKeys: String, CodingKey {
            case key, host, shift
        }

        /// Rust writes the key as `{"Letter": "K"}` or `{"Function": 12}`.
        enum KeyCodingKeys: String, CodingKey {
            case letter = "Letter"
            case function = "Function"
        }

        public init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            host = try container.decode(Bool.self, forKey: .host)
            shift = try container.decode(Bool.self, forKey: .shift)
            let key = try container.nestedContainer(keyedBy: KeyCodingKeys.self, forKey: .key)
            if let letter = try key.decodeIfPresent(String.self, forKey: .letter),
               letter.count == 1, let character = letter.first {
                self.key = .letter(character)
            } else if let number = try key.decodeIfPresent(Int.self, forKey: .function) {
                self.key = .function(number)
            } else {
                throw DecodingError.dataCorruptedError(
                    forKey: .key, in: container, debugDescription: "unknown shortcut key")
            }
        }
    }
}
