use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "goal_state", rename_all = "snake_case")]
pub enum GoalState {
    Draft,
    Inprogress,
    Reached,
    Failed,
    Canceled,
}

impl std::fmt::Display for GoalState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Draft => write!(f, "draft"),
            Self::Inprogress => write!(f, "inprogress"),
            Self::Reached => write!(f, "reached"),
            Self::Failed => write!(f, "failed"),
            Self::Canceled => write!(f, "canceled"),
        }
    }
}

impl FromStr for GoalState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "draft" => Ok(Self::Draft),
            "inprogress" => Ok(Self::Inprogress),
            "reached" => Ok(Self::Reached),
            "failed" => Ok(Self::Failed),
            "canceled" => Ok(Self::Canceled),
            _ => Err(format!("Unknown GoalState variant: {}", s)),
        }
    }
}

impl Default for GoalState {
    fn default() -> Self {
        Self::Draft
    }
}
