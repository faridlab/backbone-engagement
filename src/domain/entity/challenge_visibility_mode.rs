use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "challenge_visibility_mode", rename_all = "snake_case")]
pub enum ChallengeVisibilityMode {
    Personal,
    Ranking,
}

impl std::fmt::Display for ChallengeVisibilityMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Personal => write!(f, "personal"),
            Self::Ranking => write!(f, "ranking"),
        }
    }
}

impl FromStr for ChallengeVisibilityMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "personal" => Ok(Self::Personal),
            "ranking" => Ok(Self::Ranking),
            _ => Err(format!("Unknown ChallengeVisibilityMode variant: {}", s)),
        }
    }
}

impl Default for ChallengeVisibilityMode {
    fn default() -> Self {
        Self::Personal
    }
}
