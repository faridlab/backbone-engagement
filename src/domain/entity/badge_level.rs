use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "badge_level", rename_all = "snake_case")]
pub enum BadgeLevel {
    Bronze,
    Silver,
    Gold,
}

impl std::fmt::Display for BadgeLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bronze => write!(f, "bronze"),
            Self::Silver => write!(f, "silver"),
            Self::Gold => write!(f, "gold"),
        }
    }
}

impl FromStr for BadgeLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "bronze" => Ok(Self::Bronze),
            "silver" => Ok(Self::Silver),
            "gold" => Ok(Self::Gold),
            _ => Err(format!("Unknown BadgeLevel variant: {}", s)),
        }
    }
}

impl Default for BadgeLevel {
    fn default() -> Self {
        Self::Bronze
    }
}
