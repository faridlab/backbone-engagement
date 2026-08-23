use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for EngagementLinkTracker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngagementLinkTrackerId(pub Uuid);

impl EngagementLinkTrackerId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for EngagementLinkTrackerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for EngagementLinkTrackerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for EngagementLinkTrackerId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<EngagementLinkTrackerId> for Uuid {
    fn from(id: EngagementLinkTrackerId) -> Self { id.0 }
}

impl AsRef<Uuid> for EngagementLinkTrackerId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for EngagementLinkTrackerId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EngagementLinkTracker {
    pub id: Uuid,
    pub url: String,
    pub code: String,
    pub title: Option<String>,
    pub label: String,
    pub campaign_id: Option<Uuid>,
    pub medium_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl EngagementLinkTracker {
    /// Create a builder for EngagementLinkTracker
    pub fn builder() -> EngagementLinkTrackerBuilder {
        <EngagementLinkTrackerBuilder as Default>::default()
    }

    /// Create a new EngagementLinkTracker with required fields
    pub fn new(url: String, code: String, label: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            url,
            code,
            title: None,
            label,
            campaign_id: None,
            medium_id: None,
            source_id: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> EngagementLinkTrackerId {
        EngagementLinkTrackerId(self.id)
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

    /// Set the title field (chainable)
    pub fn with_title(mut self, value: String) -> Self {
        self.title = Some(value);
        self
    }

    /// Set the campaign_id field (chainable)
    pub fn with_campaign_id(mut self, value: Uuid) -> Self {
        self.campaign_id = Some(value);
        self
    }

    /// Set the medium_id field (chainable)
    pub fn with_medium_id(mut self, value: Uuid) -> Self {
        self.medium_id = Some(value);
        self
    }

    /// Set the source_id field (chainable)
    pub fn with_source_id(mut self, value: Uuid) -> Self {
        self.source_id = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "url" => {
                    if let Ok(v) = serde_json::from_value(value) { self.url = v; }
                }
                "code" => {
                    if let Ok(v) = serde_json::from_value(value) { self.code = v; }
                }
                "title" => {
                    if let Ok(v) = serde_json::from_value(value) { self.title = v; }
                }
                "label" => {
                    if let Ok(v) = serde_json::from_value(value) { self.label = v; }
                }
                "campaign_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.campaign_id = v; }
                }
                "medium_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.medium_id = v; }
                }
                "source_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.source_id = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for EngagementLinkTracker {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "EngagementLinkTracker"
    }
}

impl backbone_core::PersistentEntity for EngagementLinkTracker {
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

impl backbone_orm::EntityRepoMeta for EngagementLinkTracker {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("campaign_id".to_string(), "uuid".to_string());
        m.insert("medium_id".to_string(), "uuid".to_string());
        m.insert("source_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["url", "code", "label"]
    }
}

/// Builder for EngagementLinkTracker entity
///
/// Provides a fluent API for constructing EngagementLinkTracker instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct EngagementLinkTrackerBuilder {
    url: Option<String>,
    code: Option<String>,
    title: Option<String>,
    label: Option<String>,
    campaign_id: Option<Uuid>,
    medium_id: Option<Uuid>,
    source_id: Option<Uuid>,
}

impl EngagementLinkTrackerBuilder {
    /// Set the url field (required)
    pub fn url(mut self, value: String) -> Self {
        self.url = Some(value);
        self
    }

    /// Set the code field (required)
    pub fn code(mut self, value: String) -> Self {
        self.code = Some(value);
        self
    }

    /// Set the title field (optional)
    pub fn title(mut self, value: String) -> Self {
        self.title = Some(value);
        self
    }

    /// Set the label field (required)
    pub fn label(mut self, value: String) -> Self {
        self.label = Some(value);
        self
    }

    /// Set the campaign_id field (optional)
    pub fn campaign_id(mut self, value: Uuid) -> Self {
        self.campaign_id = Some(value);
        self
    }

    /// Set the medium_id field (optional)
    pub fn medium_id(mut self, value: Uuid) -> Self {
        self.medium_id = Some(value);
        self
    }

    /// Set the source_id field (optional)
    pub fn source_id(mut self, value: Uuid) -> Self {
        self.source_id = Some(value);
        self
    }

    /// Build the EngagementLinkTracker entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<EngagementLinkTracker, String> {
        let url = self.url.ok_or_else(|| "url is required".to_string())?;
        let code = self.code.ok_or_else(|| "code is required".to_string())?;
        let label = self.label.ok_or_else(|| "label is required".to_string())?;

        Ok(EngagementLinkTracker {
            id: Uuid::new_v4(),
            url,
            code,
            title: self.title,
            label,
            campaign_id: self.campaign_id,
            medium_id: self.medium_id,
            source_id: self.source_id,
            metadata: AuditMetadata::default(),
        })
    }
}
