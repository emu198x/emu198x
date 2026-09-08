//! Capability identifiers and sets.
//!
//! Capabilities are intentionally data-driven. Families advertise what they can
//! do instead of frontends assuming every machine has the same inspection or
//! teaching surfaces.

use std::borrow::Cow;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// Stable identifier for a machine capability.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CapabilityId(Cow<'static, str>);

impl CapabilityId {
    /// Creates a capability identifier from a borrowed or owned string.
    #[must_use]
    pub fn new(id: impl Into<Cow<'static, str>>) -> Self {
        Self(id.into())
    }

    /// Returns the stable string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl From<&'static str> for CapabilityId {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl From<String> for CapabilityId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

/// A set of capabilities exposed by a machine or profile.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    entries: BTreeSet<CapabilityId>,
}

impl CapabilitySet {
    /// Creates an empty capability set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a capability set from an iterator.
    #[must_use]
    pub fn with_all<I>(items: I) -> Self
    where
        I: IntoIterator<Item = CapabilityId>,
    {
        Self {
            entries: items.into_iter().collect(),
        }
    }

    /// Inserts one capability identifier.
    pub fn insert(&mut self, capability: CapabilityId) {
        self.entries.insert(capability);
    }

    /// Returns `true` if the set contains the capability.
    #[must_use]
    pub fn contains(&self, capability: &CapabilityId) -> bool {
        self.entries.contains(capability)
    }

    /// Returns an iterator over all capabilities in sorted order.
    pub fn iter(&self) -> impl Iterator<Item = &CapabilityId> {
        self.entries.iter()
    }

    /// Returns `true` if the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of capabilities in the set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// Returns a well-known capability identifier.
/// Capability ids the shell's MCP registrar reads from a machine profile.
///
/// A profile declares what the *machine* has; the registrar registers the
/// tool tier for it, whether or not the live target exists yet (a machine
/// that starts blank grows its targets when firmware loads, and the tools
/// must already be there for the client to load it). The executor still
/// checks the live target on every call, so a declared-but-not-yet-loaded
/// capability fails with the capability-missing error, never silently.
pub mod ids {
    /// A typewriter keyboard the shell's `KeyboardTarget` verbs drive.
    pub const KEYBOARD_INPUT: &str = "keyboard-input";
    /// The older spelling some families use for the same thing.
    pub const KEYBOARD_MATRIX: &str = "keyboard-matrix";
    /// An AY-3-891x sound chip: the `watch_ay_*` tier.
    pub const AY_AUDIO: &str = "ay-audio";
    /// Memory-write capture through `WatchTarget`: the `watch_memory_*` tier.
    pub const MEMORY_WATCH: &str = "memory-watch";
    /// A CPU port space through `PortIoTarget`: `port_read` / `port_write`.
    pub const PORT_IO: &str = "port-io";
    /// The machine can take a plain-text BASIC listing straight into RAM.
    pub const BASIC_PROGRAM_LOAD: &str = "basic-program-load";
    /// The machine can type its own tape-load command and start the tape.
    pub const TAPE_AUTOLOAD: &str = "tape-autoload";
    /// The machine is one variant of a family and can be swapped for another live.
    pub const VARIANT_SWITCH: &str = "variant-switch";
}

#[must_use]
pub fn known_capability(id: &'static str) -> CapabilityId {
    CapabilityId::from(id)
}
