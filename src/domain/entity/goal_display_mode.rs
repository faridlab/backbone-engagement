use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "goal_display_mode", rename_all = "snake_case")]
pub enum GoalDisplayMode {
    Progress,
    Boolean,
}

impl std::fmt::Display for GoalDisplayMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Progress => write!(f, "progress"),
            Self::Boolean => write!(f, "boolean"),
        }
    }
}

impl FromStr for GoalDisplayMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "progress" => Ok(Self::Progress),
            "boolean" => Ok(Self::Boolean),
            _ => Err(format!("Unknown GoalDisplayMode variant: {}", s)),
        }
    }
}

impl Default for GoalDisplayMode {
    fn default() -> Self {
        Self::Progress
    }
}
