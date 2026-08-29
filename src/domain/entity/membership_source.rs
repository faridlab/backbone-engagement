use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "membership_source", rename_all = "snake_case")]
pub enum MembershipSource {
    Explicit,
    RuleExpansion,
}

impl std::fmt::Display for MembershipSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Explicit => write!(f, "explicit"),
            Self::RuleExpansion => write!(f, "rule_expansion"),
        }
    }
}

impl FromStr for MembershipSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "explicit" => Ok(Self::Explicit),
            "rule_expansion" => Ok(Self::RuleExpansion),
            _ => Err(format!("Unknown MembershipSource variant: {}", s)),
        }
    }
}

impl Default for MembershipSource {
    fn default() -> Self {
        Self::Explicit
    }
}
