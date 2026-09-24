//! Edge capture and eligibility for the NMI boundary decision.
//!
//! The die-derived arrival sweep puts the deadline two half-cycles before
//! the next T1 rising edge. Retain short pulses arriving after that deadline
//! for a later boundary. This models eligibility, not internal node voltages.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) enum NmiLatch {
    #[default]
    Empty,
    Ready,
    OneHalfcycle,
    TwoHalfcycles,
}

impl NmiLatch {
    pub(crate) fn tick(&mut self, edge: bool) {
        *self = match *self {
            Self::TwoHalfcycles => Self::OneHalfcycle,
            Self::OneHalfcycle => Self::Ready,
            state => state,
        };
        // A second edge cannot postpone an already captured request.
        if edge && *self == Self::Empty {
            *self = Self::TwoHalfcycles;
        }
    }

    pub(crate) fn take_ready(&mut self) -> bool {
        if *self == Self::Ready {
            *self = Self::Empty;
            true
        } else {
            false
        }
    }
}

// Preserve the old bool's binary 0/1 encoding and field position. Existing
// snapshots map false to empty and true to already eligible; new in-flight
// states use 2/3. Human-readable snapshots also accept the old bool values.
impl Serialize for NmiLatch {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(match self {
            Self::Empty => 0,
            Self::Ready => 1,
            Self::OneHalfcycle => 2,
            Self::TwoHalfcycles => 3,
        })
    }
}

impl<'de> Deserialize<'de> for NmiLatch {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = NmiLatch;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an NMI latch state from 0 to 3 or a legacy boolean")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(if value {
                    NmiLatch::Ready
                } else {
                    NmiLatch::Empty
                })
            }
            fn visit_u8<E: de::Error>(self, value: u8) -> Result<Self::Value, E> {
                self.visit_u64(u64::from(value))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                match value {
                    0 => Ok(NmiLatch::Empty),
                    1 => Ok(NmiLatch::Ready),
                    2 => Ok(NmiLatch::OneHalfcycle),
                    3 => Ok(NmiLatch::TwoHalfcycles),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &self)),
                }
            }
        }
        if deserializer.is_human_readable() {
            deserializer.deserialize_any(Visitor)
        } else {
            deserializer.deserialize_u8(Visitor)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NmiLatch;

    #[test]
    fn legacy_boolean_encoding_preserves_following_fields() {
        for (old, expected) in [(false, NmiLatch::Empty), (true, NmiLatch::Ready)] {
            let bytes = postcard::to_allocvec(&(0x55u8, old, 0xaau8)).expect("encode legacy");
            let decoded: (u8, NmiLatch, u8) = postcard::from_bytes(&bytes).expect("decode legacy");
            assert_eq!(decoded, (0x55, expected, 0xaa));
            assert_eq!(postcard::to_allocvec(&decoded).expect("encode"), bytes);
            assert_eq!(
                serde_json::from_str::<NmiLatch>(if old { "true" } else { "false" })
                    .expect("legacy JSON"),
                expected
            );
        }
        assert!(postcard::from_bytes::<NmiLatch>(&[4]).is_err());
        assert!(serde_json::from_str::<NmiLatch>("4").is_err());
    }

    #[test]
    fn additional_edge_does_not_postpone_pending_request() {
        let mut latch = NmiLatch::Empty;
        latch.tick(true);
        assert!(!latch.take_ready());
        latch.tick(false);
        assert!(!latch.take_ready());
        latch.tick(true);
        assert!(latch.take_ready());
        assert!(!latch.take_ready());
    }
}
