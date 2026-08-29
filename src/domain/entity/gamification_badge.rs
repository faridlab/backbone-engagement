use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::BadgeLevel;
use super::BadgeRuleAuth;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationBadge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationBadgeId(pub Uuid);

impl GamificationBadgeId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationBadgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationBadgeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationBadgeId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationBadgeId> for Uuid {
    fn from(id: GamificationBadgeId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationBadgeId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationBadgeId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationBadge {
    pub id: Uuid,
    pub name: String,
    pub active: bool,
    pub level: Option<BadgeLevel>,
    pub description: Option<String>,
    pub rule_auth: BadgeRuleAuth,
    pub rule_auth_user_ids: serde_json::Value,
    pub rule_auth_badge_ids: serde_json::Value,
    pub rule_max: bool,
    pub rule_max_number: Option<i32>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationBadge {
    /// Create a builder for GamificationBadge
    pub fn builder() -> GamificationBadgeBuilder {
        <GamificationBadgeBuilder as Default>::default()
    }

    /// Create a new GamificationBadge with required fields
    pub fn new(name: String, active: bool, rule_auth: BadgeRuleAuth, rule_auth_user_ids: serde_json::Value, rule_auth_badge_ids: serde_json::Value, rule_max: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            active,
            level: None,
            description: None,
            rule_auth,
            rule_auth_user_ids,
            rule_auth_badge_ids,
            rule_max,
            rule_max_number: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationBadgeId {
        GamificationBadgeId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }


    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the level field (chainable)
    pub fn with_level(mut self, value: BadgeLevel) -> Self {
        self.level = Some(value);
        self
    }

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the rule_max_number field (chainable)
    pub fn with_rule_max_number(mut self, value: i32) -> Self {
        self.rule_max_number = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.active = v; }
                }
                "level" => {
                    if let Ok(v) = serde_json::from_value(value) { self.level = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "rule_auth" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rule_auth = v; }
                }
                "rule_auth_user_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rule_auth_user_ids = v; }
                }
                "rule_auth_badge_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rule_auth_badge_ids = v; }
                }
                "rule_max" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rule_max = v; }
                }
                "rule_max_number" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rule_max_number = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationBadge {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationBadge"
    }
}

impl backbone_core::PersistentEntity for GamificationBadge {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for GamificationBadge {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("level".to_string(), "badge_level".to_string());
        m.insert("rule_auth".to_string(), "badge_rule_auth".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for GamificationBadge entity
///
/// Provides a fluent API for constructing GamificationBadge instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationBadgeBuilder {
    name: Option<String>,
    active: Option<bool>,
    level: Option<BadgeLevel>,
    description: Option<String>,
    rule_auth: Option<BadgeRuleAuth>,
    rule_auth_user_ids: Option<serde_json::Value>,
    rule_auth_badge_ids: Option<serde_json::Value>,
    rule_max: Option<bool>,
    rule_max_number: Option<i32>,
}

impl GamificationBadgeBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the active field (default: `true`)
    pub fn active(mut self, value: bool) -> Self {
        self.active = Some(value);
        self
    }

    /// Set the level field (optional)
    pub fn level(mut self, value: BadgeLevel) -> Self {
        self.level = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the rule_auth field (required)
    pub fn rule_auth(mut self, value: BadgeRuleAuth) -> Self {
        self.rule_auth = Some(value);
        self
    }

    /// Set the rule_auth_user_ids field (default: `serde_json::json!([])`)
    pub fn rule_auth_user_ids(mut self, value: serde_json::Value) -> Self {
        self.rule_auth_user_ids = Some(value);
        self
    }

    /// Set the rule_auth_badge_ids field (default: `serde_json::json!([])`)
    pub fn rule_auth_badge_ids(mut self, value: serde_json::Value) -> Self {
        self.rule_auth_badge_ids = Some(value);
        self
    }

    /// Set the rule_max field (default: `false`)
    pub fn rule_max(mut self, value: bool) -> Self {
        self.rule_max = Some(value);
        self
    }

    /// Set the rule_max_number field (optional)
    pub fn rule_max_number(mut self, value: i32) -> Self {
        self.rule_max_number = Some(value);
        self
    }

    /// Build the GamificationBadge entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationBadge, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;
        let rule_auth = self.rule_auth.ok_or_else(|| "rule_auth is required".to_string())?;

        Ok(GamificationBadge {
            id: Uuid::new_v4(),
            name,
            active: self.active.unwrap_or(true),
            level: self.level,
            description: self.description,
            rule_auth,
            rule_auth_user_ids: self.rule_auth_user_ids.unwrap_or(serde_json::json!([])),
            rule_auth_badge_ids: self.rule_auth_badge_ids.unwrap_or(serde_json::json!([])),
            rule_max: self.rule_max.unwrap_or(false),
            rule_max_number: self.rule_max_number,
            metadata: AuditMetadata::default(),
        })
    }
}
