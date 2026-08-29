use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationChallengeLine
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationChallengeLineId(pub Uuid);

impl GamificationChallengeLineId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationChallengeLineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationChallengeLineId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationChallengeLineId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationChallengeLineId> for Uuid {
    fn from(id: GamificationChallengeLineId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationChallengeLineId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationChallengeLineId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationChallengeLine {
    pub id: Uuid,
    pub challenge_id: Uuid,
    pub definition_id: Uuid,
    pub sequence: i32,
    pub target_goal: Decimal,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationChallengeLine {
    /// Create a builder for GamificationChallengeLine
    pub fn builder() -> GamificationChallengeLineBuilder {
        <GamificationChallengeLineBuilder as Default>::default()
    }

    /// Create a new GamificationChallengeLine with required fields
    pub fn new(challenge_id: Uuid, definition_id: Uuid, sequence: i32, target_goal: Decimal) -> Self {
        Self {
            id: Uuid::new_v4(),
            challenge_id,
            definition_id,
            sequence,
            target_goal,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationChallengeLineId {
        GamificationChallengeLineId(self.id)
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
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "challenge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.challenge_id = v; }
                }
                "definition_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.definition_id = v; }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "target_goal" => {
                    if let Ok(v) = serde_json::from_value(value) { self.target_goal = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationChallengeLine {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationChallengeLine"
    }
}

impl backbone_core::PersistentEntity for GamificationChallengeLine {
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

impl backbone_orm::EntityRepoMeta for GamificationChallengeLine {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("challenge_id".to_string(), "uuid".to_string());
        m.insert("definition_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for GamificationChallengeLine entity
///
/// Provides a fluent API for constructing GamificationChallengeLine instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationChallengeLineBuilder {
    challenge_id: Option<Uuid>,
    definition_id: Option<Uuid>,
    sequence: Option<i32>,
    target_goal: Option<Decimal>,
}

impl GamificationChallengeLineBuilder {
    /// Set the challenge_id field (required)
    pub fn challenge_id(mut self, value: Uuid) -> Self {
        self.challenge_id = Some(value);
        self
    }

    /// Set the definition_id field (required)
    pub fn definition_id(mut self, value: Uuid) -> Self {
        self.definition_id = Some(value);
        self
    }

    /// Set the sequence field (default: `1`)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the target_goal field (required)
    pub fn target_goal(mut self, value: Decimal) -> Self {
        self.target_goal = Some(value);
        self
    }

    /// Build the GamificationChallengeLine entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationChallengeLine, String> {
        let challenge_id = self.challenge_id.ok_or_else(|| "challenge_id is required".to_string())?;
        let definition_id = self.definition_id.ok_or_else(|| "definition_id is required".to_string())?;
        let target_goal = self.target_goal.ok_or_else(|| "target_goal is required".to_string())?;

        Ok(GamificationChallengeLine {
            id: Uuid::new_v4(),
            challenge_id,
            definition_id,
            sequence: self.sequence.unwrap_or(1),
            target_goal,
            metadata: AuditMetadata::default(),
        })
    }
}
