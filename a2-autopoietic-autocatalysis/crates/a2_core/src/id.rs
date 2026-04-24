use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

fn deterministic_uuid(prefix: &str, key: &str) -> Uuid {
    // FNV-1a 128-bit over the typed-ID prefix and external key. The UUID is
    // marked as v8 (custom) with RFC 4122 variant bits so arbitrary external
    // task keys can become stable typed IDs without a new dependency.
    let mut hash = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d_u128;
    for byte in prefix.bytes().chain([0]).chain(key.bytes()) {
        hash ^= u128::from(byte);
        hash = hash.wrapping_mul(0x0000_0000_0100_0000_0000_0000_0000_013b_u128);
    }

    let mut bytes = hash.to_be_bytes();
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

macro_rules! typed_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
        pub struct $name(Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn as_uuid(&self) -> &Uuid {
                &self.0
            }

            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub fn parse_str(value: &str) -> Result<Self, uuid::Error> {
                let raw = value.strip_prefix(concat!($prefix, "-")).unwrap_or(value);
                Uuid::parse_str(raw).map(Self)
            }

            pub fn from_external_key(key: &str) -> Self {
                Self(deterministic_uuid($prefix, key))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}-{}", $prefix, self.0)
            }
        }
    };
}

typed_id!(TaskId, "task");
typed_id!(WorkcellId, "wc");
typed_id!(PatchId, "patch");
typed_id!(LineageId, "lin");
typed_id!(PromotionId, "promo");
typed_id!(EvalId, "eval");
typed_id!(CatalystId, "cat");
typed_id!(GermlineVersion, "gv");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_display_and_raw_uuid_forms() {
        let original = TaskId::new();

        assert_eq!(TaskId::parse_str(&original.to_string()).unwrap(), original);
        assert_eq!(
            TaskId::parse_str(&original.as_uuid().to_string()).unwrap(),
            original
        );
    }

    #[test]
    fn external_keys_map_to_stable_typed_ids() {
        assert_eq!(
            TaskId::from_external_key("bench-1"),
            TaskId::from_external_key("bench-1")
        );
        assert_ne!(
            TaskId::from_external_key("bench-1"),
            TaskId::from_external_key("bench-2")
        );
        assert_ne!(
            TaskId::from_external_key("bench-1").as_uuid(),
            WorkcellId::from_external_key("bench-1").as_uuid()
        );
    }
}
