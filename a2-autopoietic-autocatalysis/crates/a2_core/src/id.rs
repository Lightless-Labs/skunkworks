use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

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
