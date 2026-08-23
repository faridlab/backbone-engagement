use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for EngagementLinkTrackerClick
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngagementLinkTrackerClickId(pub Uuid);

impl EngagementLinkTrackerClickId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for EngagementLinkTrackerClickId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for EngagementLinkTrackerClickId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for EngagementLinkTrackerClickId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<EngagementLinkTrackerClickId> for Uuid {
    fn from(id: EngagementLinkTrackerClickId) -> Self { id.0 }
}

impl AsRef<Uuid> for EngagementLinkTrackerClickId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for EngagementLinkTrackerClickId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EngagementLinkTrackerClick {
    pub id: Uuid,
    pub link_id: Uuid,
    pub campaign_id: Option<Uuid>,
    pub ip: Option<String>,
    pub country_code: Option<String>,
    pub click_day: NaiveDate,
    pub dedup_key: String,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl EngagementLinkTrackerClick {
    /// Create a builder for EngagementLinkTrackerClick
    pub fn builder() -> EngagementLinkTrackerClickBuilder {
        <EngagementLinkTrackerClickBuilder as Default>::default()
    }

    /// Create a new EngagementLinkTrackerClick with required fields
    pub fn new(link_id: Uuid, click_day: NaiveDate, dedup_key: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            link_id,
            campaign_id: None,
            ip: None,
            country_code: None,
            click_day,
            dedup_key,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> EngagementLinkTrackerClickId {
        EngagementLinkTrackerClickId(self.id)
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

    /// Set the campaign_id field (chainable)
    pub fn with_campaign_id(mut self, value: Uuid) -> Self {
        self.campaign_id = Some(value);
        self
    }

    /// Set the ip field (chainable)
    pub fn with_ip(mut self, value: String) -> Self {
        self.ip = Some(value);
        self
    }

    /// Set the country_code field (chainable)
    pub fn with_country_code(mut self, value: String) -> Self {
        self.country_code = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "link_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.link_id = v; }
                }
                "campaign_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.campaign_id = v; }
                }
                "ip" => {
                    if let Ok(v) = serde_json::from_value(value) { self.ip = v; }
                }
                "country_code" => {
                    if let Ok(v) = serde_json::from_value(value) { self.country_code = v; }
                }
                "click_day" => {
                    if let Ok(v) = serde_json::from_value(value) { self.click_day = v; }
                }
                "dedup_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.dedup_key = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for EngagementLinkTrackerClick {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "EngagementLinkTrackerClick"
    }
}

impl backbone_core::PersistentEntity for EngagementLinkTrackerClick {
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

impl backbone_orm::EntityRepoMeta for EngagementLinkTrackerClick {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("link_id".to_string(), "uuid".to_string());
        m.insert("campaign_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["dedup_key"]
    }
}

/// Builder for EngagementLinkTrackerClick entity
///
/// Provides a fluent API for constructing EngagementLinkTrackerClick instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct EngagementLinkTrackerClickBuilder {
    link_id: Option<Uuid>,
    campaign_id: Option<Uuid>,
    ip: Option<String>,
    country_code: Option<String>,
    click_day: Option<NaiveDate>,
    dedup_key: Option<String>,
}

impl EngagementLinkTrackerClickBuilder {
    /// Set the link_id field (required)
    pub fn link_id(mut self, value: Uuid) -> Self {
        self.link_id = Some(value);
        self
    }

    /// Set the campaign_id field (optional)
    pub fn campaign_id(mut self, value: Uuid) -> Self {
        self.campaign_id = Some(value);
        self
    }

    /// Set the ip field (optional)
    pub fn ip(mut self, value: String) -> Self {
        self.ip = Some(value);
        self
    }

    /// Set the country_code field (optional)
    pub fn country_code(mut self, value: String) -> Self {
        self.country_code = Some(value);
        self
    }

    /// Set the click_day field (required)
    pub fn click_day(mut self, value: NaiveDate) -> Self {
        self.click_day = Some(value);
        self
    }

    /// Set the dedup_key field (required)
    pub fn dedup_key(mut self, value: String) -> Self {
        self.dedup_key = Some(value);
        self
    }

    /// Build the EngagementLinkTrackerClick entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<EngagementLinkTrackerClick, String> {
        let link_id = self.link_id.ok_or_else(|| "link_id is required".to_string())?;
        let click_day = self.click_day.ok_or_else(|| "click_day is required".to_string())?;
        let dedup_key = self.dedup_key.ok_or_else(|| "dedup_key is required".to_string())?;

        Ok(EngagementLinkTrackerClick {
            id: Uuid::new_v4(),
            link_id,
            campaign_id: self.campaign_id,
            ip: self.ip,
            country_code: self.country_code,
            click_day,
            dedup_key,
            metadata: AuditMetadata::default(),
        })
    }
}
