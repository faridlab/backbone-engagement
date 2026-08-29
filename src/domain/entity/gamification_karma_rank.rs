use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationKarmaRank
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationKarmaRankId(pub Uuid);

impl GamificationKarmaRankId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationKarmaRankId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationKarmaRankId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationKarmaRankId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationKarmaRankId> for Uuid {
    fn from(id: GamificationKarmaRankId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationKarmaRankId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationKarmaRankId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationKarmaRank {
    pub id: Uuid,
    pub name: String,
    pub karma_min: i32,
    pub description: Option<String>,
    pub motivational_text: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationKarmaRank {
    /// Create a builder for GamificationKarmaRank
    pub fn builder() -> GamificationKarmaRankBuilder {
        <GamificationKarmaRankBuilder as Default>::default()
    }

    /// Create a new GamificationKarmaRank with required fields
    pub fn new(name: String, karma_min: i32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            karma_min,
            description: None,
            motivational_text: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationKarmaRankId {
        GamificationKarmaRankId(self.id)
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

    /// Set the description field (chainable)
    pub fn with_description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the motivational_text field (chainable)
    pub fn with_motivational_text(mut self, value: String) -> Self {
        self.motivational_text = Some(value);
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
                "karma_min" => {
                    if let Ok(v) = serde_json::from_value(value) { self.karma_min = v; }
                }
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "motivational_text" => {
                    if let Ok(v) = serde_json::from_value(value) { self.motivational_text = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationKarmaRank {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationKarmaRank"
    }
}

impl backbone_core::PersistentEntity for GamificationKarmaRank {
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

impl backbone_orm::EntityRepoMeta for GamificationKarmaRank {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for GamificationKarmaRank entity
///
/// Provides a fluent API for constructing GamificationKarmaRank instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationKarmaRankBuilder {
    name: Option<String>,
    karma_min: Option<i32>,
    description: Option<String>,
    motivational_text: Option<String>,
}

impl GamificationKarmaRankBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the karma_min field (default: `1`)
    pub fn karma_min(mut self, value: i32) -> Self {
        self.karma_min = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the motivational_text field (optional)
    pub fn motivational_text(mut self, value: String) -> Self {
        self.motivational_text = Some(value);
        self
    }

    /// Build the GamificationKarmaRank entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationKarmaRank, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(GamificationKarmaRank {
            id: Uuid::new_v4(),
            name,
            karma_min: self.karma_min.unwrap_or(1),
            description: self.description,
            motivational_text: self.motivational_text,
            metadata: AuditMetadata::default(),
        })
    }
}
