use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationKarmaTracking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationKarmaTrackingId(pub Uuid);

impl GamificationKarmaTrackingId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationKarmaTrackingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationKarmaTrackingId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationKarmaTrackingId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationKarmaTrackingId> for Uuid {
    fn from(id: GamificationKarmaTrackingId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationKarmaTrackingId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationKarmaTrackingId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationKarmaTracking {
    pub id: Uuid,
    pub user_id: Uuid,
    pub old_value: i32,
    pub new_value: i32,
    pub tracking_date: DateTime<Utc>,
    pub reason: String,
    pub origin_kind: String,
    pub origin_id: Option<Uuid>,
    pub origin_label: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationKarmaTracking {
    /// Create a builder for GamificationKarmaTracking
    pub fn builder() -> GamificationKarmaTrackingBuilder {
        <GamificationKarmaTrackingBuilder as Default>::default()
    }

    /// Create a new GamificationKarmaTracking with required fields
    pub fn new(user_id: Uuid, old_value: i32, new_value: i32, tracking_date: DateTime<Utc>, reason: String, origin_kind: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            old_value,
            new_value,
            tracking_date,
            reason,
            origin_kind,
            origin_id: None,
            origin_label: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationKarmaTrackingId {
        GamificationKarmaTrackingId(self.id)
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

    /// Set the origin_id field (chainable)
    pub fn with_origin_id(mut self, value: Uuid) -> Self {
        self.origin_id = Some(value);
        self
    }

    /// Set the origin_label field (chainable)
    pub fn with_origin_label(mut self, value: String) -> Self {
        self.origin_label = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "old_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.old_value = v; }
                }
                "new_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.new_value = v; }
                }
                "tracking_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.tracking_date = v; }
                }
                "reason" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reason = v; }
                }
                "origin_kind" => {
                    if let Ok(v) = serde_json::from_value(value) { self.origin_kind = v; }
                }
                "origin_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.origin_id = v; }
                }
                "origin_label" => {
                    if let Ok(v) = serde_json::from_value(value) { self.origin_label = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationKarmaTracking {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationKarmaTracking"
    }
}

impl backbone_core::PersistentEntity for GamificationKarmaTracking {
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

impl backbone_orm::EntityRepoMeta for GamificationKarmaTracking {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("origin_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["reason", "origin_kind"]
    }
}

/// Builder for GamificationKarmaTracking entity
///
/// Provides a fluent API for constructing GamificationKarmaTracking instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationKarmaTrackingBuilder {
    user_id: Option<Uuid>,
    old_value: Option<i32>,
    new_value: Option<i32>,
    tracking_date: Option<DateTime<Utc>>,
    reason: Option<String>,
    origin_kind: Option<String>,
    origin_id: Option<Uuid>,
    origin_label: Option<String>,
}

impl GamificationKarmaTrackingBuilder {
    /// Set the user_id field (required)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the old_value field (required)
    pub fn old_value(mut self, value: i32) -> Self {
        self.old_value = Some(value);
        self
    }

    /// Set the new_value field (required)
    pub fn new_value(mut self, value: i32) -> Self {
        self.new_value = Some(value);
        self
    }

    /// Set the tracking_date field (default: `Utc::now()`)
    pub fn tracking_date(mut self, value: DateTime<Utc>) -> Self {
        self.tracking_date = Some(value);
        self
    }

    /// Set the reason field (required)
    pub fn reason(mut self, value: String) -> Self {
        self.reason = Some(value);
        self
    }

    /// Set the origin_kind field (required)
    pub fn origin_kind(mut self, value: String) -> Self {
        self.origin_kind = Some(value);
        self
    }

    /// Set the origin_id field (optional)
    pub fn origin_id(mut self, value: Uuid) -> Self {
        self.origin_id = Some(value);
        self
    }

    /// Set the origin_label field (optional)
    pub fn origin_label(mut self, value: String) -> Self {
        self.origin_label = Some(value);
        self
    }

    /// Build the GamificationKarmaTracking entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationKarmaTracking, String> {
        let user_id = self.user_id.ok_or_else(|| "user_id is required".to_string())?;
        let old_value = self.old_value.ok_or_else(|| "old_value is required".to_string())?;
        let new_value = self.new_value.ok_or_else(|| "new_value is required".to_string())?;
        let reason = self.reason.ok_or_else(|| "reason is required".to_string())?;
        let origin_kind = self.origin_kind.ok_or_else(|| "origin_kind is required".to_string())?;

        Ok(GamificationKarmaTracking {
            id: Uuid::new_v4(),
            user_id,
            old_value,
            new_value,
            tracking_date: self.tracking_date.unwrap_or(Utc::now()),
            reason,
            origin_kind,
            origin_id: self.origin_id,
            origin_label: self.origin_label,
            metadata: AuditMetadata::default(),
        })
    }
}
