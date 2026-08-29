use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "goal_computation_mode", rename_all = "snake_case")]
pub enum GoalComputationMode {
    Manual,
    RegisteredMetric,
}

impl std::fmt::Display for GoalComputationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manual => write!(f, "manual"),
            Self::RegisteredMetric => write!(f, "registered_metric"),
        }
    }
}

impl FromStr for GoalComputationMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "manual" => Ok(Self::Manual),
            "registered_metric" => Ok(Self::RegisteredMetric),
            _ => Err(format!("Unknown GoalComputationMode variant: {}", s)),
        }
    }
}

impl Default for GoalComputationMode {
    fn default() -> Self {
        Self::Manual
    }
}
