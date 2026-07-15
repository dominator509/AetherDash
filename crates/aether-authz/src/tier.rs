use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The closed permission-tier set from SPEC-005.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Tier {
    ReadOnly = 1,
    DraftOnly = 2,
    ConfirmEveryAction = 3,
    BoundedAutopilot = 4,
    YoloWithinHardCaps = 5,
}

impl Tier {
    #[must_use]
    pub const fn number(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub const fn lower(self, other: Self) -> Self {
        if self.number() <= other.number() {
            self
        } else {
            other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("permission tier must be in the closed range 1..=5")]
pub struct TierError;

impl TryFrom<u8> for Tier {
    type Error = TierError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(Self::ReadOnly),
            2 => Ok(Self::DraftOnly),
            3 => Ok(Self::ConfirmEveryAction),
            4 => Ok(Self::BoundedAutopilot),
            5 => Ok(Self::YoloWithinHardCaps),
            _ => Err(TierError),
        }
    }
}
