//! External tape EAR line state shared by every 48K-class Spectrum.
//!
//! The actual `$FE` port logic lives in the Ferranti ULA crate. This type
//! captures an external tape input override for cases where the machine is
//! being driven by something other than the built-in tape player. Lives in
//! the layer crate because it's part of the 48K-class composition; the 48K
//! machine crate re-exports it for callers that import it from there today.

/// External tape EAR line state visible to port `$FE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TapeInput {
    connected: bool,
    level: bool,
}

impl TapeInput {
    /// Creates a disconnected tape input line.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns whether the tape signal is connected.
    #[must_use]
    pub const fn connected(self) -> bool {
        self.connected
    }

    /// Returns the current tape EAR level.
    #[must_use]
    pub const fn level(self) -> bool {
        self.level
    }

    /// Sets whether the tape signal is connected.
    pub fn set_connected(&mut self, connected: bool) {
        self.connected = connected;
    }

    /// Sets the current tape EAR level.
    pub fn set_level(&mut self, level: bool) {
        self.level = level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tape_input_defaults_to_disconnected_low() {
        let tape = TapeInput::new();

        assert!(!tape.connected());
        assert!(!tape.level());
    }

    #[test]
    fn tape_input_tracks_connection_and_level() {
        let mut tape = TapeInput::new();

        tape.set_connected(true);
        tape.set_level(false);
        assert!(tape.connected());
        assert!(!tape.level());

        tape.set_level(true);
        assert!(tape.level());
    }
}
