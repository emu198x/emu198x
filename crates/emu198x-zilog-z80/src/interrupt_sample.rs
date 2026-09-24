//! Boundary scheduling and IRQ level history.
//!
//! IRQ is sampled at the rising edge of the retiring instruction's final
//! T-state, two half-cycles before the next T1 rising edge dispatches a response.
//! Keeping level history preserves pulses covering that instant without
//! turning IRQ into an edge-triggered request.
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// The low bit retains the legacy `interrupt_sample_pending` boolean. Bits
/// one and two retain the IRQ levels from the previous two CPU ticks.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct InterruptSample(u8);

impl InterruptSample {
    pub(crate) fn advance_irq(&mut self, level: bool) -> bool {
        let sampled = self.0 & 4 != 0;
        self.0 = (self.0 & 1) | ((self.0 & 2) << 1) | (u8::from(level) << 1);
        sampled
    }

    pub(crate) fn arm_boundary(&mut self) {
        self.0 |= 1;
    }

    pub(crate) fn take_boundary(&mut self) -> bool {
        let pending = self.0 & 1 != 0;
        self.0 &= !1;
        pending
    }
}

impl Serialize for InterruptSample {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for InterruptSample {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = InterruptSample;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an interrupt sample from 0 to 7 or a legacy boolean")
            }
            fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(InterruptSample(u8::from(value)))
            }
            fn visit_u8<E: de::Error>(self, value: u8) -> Result<Self::Value, E> {
                self.visit_u64(u64::from(value))
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                if value <= 7 {
                    Ok(InterruptSample(value as u8))
                } else {
                    Err(E::invalid_value(de::Unexpected::Unsigned(value), &self))
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
    use super::InterruptSample;

    #[test]
    fn legacy_boundary_boolean_preserves_binary_layout_and_json_readability() {
        for old in [false, true] {
            let bytes = postcard::to_allocvec(&(0x55u8, old, 0xaau8)).expect("encode");
            let decoded: (u8, InterruptSample, u8) = postcard::from_bytes(&bytes).expect("decode");
            assert_eq!(decoded, (0x55, InterruptSample(u8::from(old)), 0xaa));
            assert_eq!(postcard::to_allocvec(&decoded).expect("encode"), bytes);
            assert_eq!(
                serde_json::from_str::<InterruptSample>(if old { "true" } else { "false" })
                    .expect("legacy JSON"),
                decoded.1
            );
        }
        assert!(postcard::from_bytes::<InterruptSample>(&[8]).is_err());
        assert!(serde_json::from_str::<InterruptSample>("8").is_err());
    }
}
