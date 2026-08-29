use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "badge_rule_auth", rename_all = "snake_case")]
pub enum BadgeRuleAuth {
    Everyone,
    Users,
    Having,
    Nobody,
}

impl std::fmt::Display for BadgeRuleAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Everyone => write!(f, "everyone"),
            Self::Users => write!(f, "users"),
            Self::Having => write!(f, "having"),
            Self::Nobody => write!(f, "nobody"),
        }
    }
}

impl FromStr for BadgeRuleAuth {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "everyone" => Ok(Self::Everyone),
            "users" => Ok(Self::Users),
            "having" => Ok(Self::Having),
            "nobody" => Ok(Self::Nobody),
            _ => Err(format!("Unknown BadgeRuleAuth variant: {}", s)),
        }
    }
}

impl Default for BadgeRuleAuth {
    fn default() -> Self {
        Self::Everyone
    }
}
