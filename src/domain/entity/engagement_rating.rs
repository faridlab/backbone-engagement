use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for EngagementRating
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct EngagementRatingId(pub Uuid);

impl EngagementRatingId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for EngagementRatingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for EngagementRatingId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for EngagementRatingId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<EngagementRatingId> for Uuid {
    fn from(id: EngagementRatingId) -> Self { id.0 }
}

impl AsRef<Uuid> for EngagementRatingId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for EngagementRatingId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EngagementRating {
    pub id: Uuid,
    pub rated_model: String,
    pub rated_record_id: Uuid,
    pub parent_rated_model: Option<String>,
    pub parent_rated_record_id: Option<Uuid>,
    pub rated_user_id: Option<Uuid>,
    pub rater_user_id: Option<Uuid>,
    pub rater_email: Option<String>,
    pub rating_value: Option<i32>,
    pub feedback: Option<String>,
    pub consumed: bool,
    pub rated_on: Option<DateTime<Utc>>,
    pub token_nonce: String,
    pub token_expires_at: DateTime<Utc>,
    pub publisher_comment: Option<String>,
    pub publisher_user_id: Option<Uuid>,
    pub publisher_replied_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl EngagementRating {
    /// Create a builder for EngagementRating
    pub fn builder() -> EngagementRatingBuilder {
        <EngagementRatingBuilder as Default>::default()
    }

    /// Create a new EngagementRating with required fields
    pub fn new(rated_model: String, rated_record_id: Uuid, consumed: bool, token_nonce: String, token_expires_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            rated_model,
            rated_record_id,
            parent_rated_model: None,
            parent_rated_record_id: None,
            rated_user_id: None,
            rater_user_id: None,
            rater_email: None,
            rating_value: None,
            feedback: None,
            consumed,
            rated_on: None,
            token_nonce,
            token_expires_at,
            publisher_comment: None,
            publisher_user_id: None,
            publisher_replied_at: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> EngagementRatingId {
        EngagementRatingId(self.id)
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

    /// Set the parent_rated_model field (chainable)
    pub fn with_parent_rated_model(mut self, value: String) -> Self {
        self.parent_rated_model = Some(value);
        self
    }

    /// Set the parent_rated_record_id field (chainable)
    pub fn with_parent_rated_record_id(mut self, value: Uuid) -> Self {
        self.parent_rated_record_id = Some(value);
        self
    }

    /// Set the rated_user_id field (chainable)
    pub fn with_rated_user_id(mut self, value: Uuid) -> Self {
        self.rated_user_id = Some(value);
        self
    }

    /// Set the rater_user_id field (chainable)
    pub fn with_rater_user_id(mut self, value: Uuid) -> Self {
        self.rater_user_id = Some(value);
        self
    }

    /// Set the rater_email field (chainable)
    pub fn with_rater_email(mut self, value: String) -> Self {
        self.rater_email = Some(value);
        self
    }

    /// Set the rating_value field (chainable)
    pub fn with_rating_value(mut self, value: i32) -> Self {
        self.rating_value = Some(value);
        self
    }

    /// Set the feedback field (chainable)
    pub fn with_feedback(mut self, value: String) -> Self {
        self.feedback = Some(value);
        self
    }

    /// Set the rated_on field (chainable)
    pub fn with_rated_on(mut self, value: DateTime<Utc>) -> Self {
        self.rated_on = Some(value);
        self
    }

    /// Set the publisher_comment field (chainable)
    pub fn with_publisher_comment(mut self, value: String) -> Self {
        self.publisher_comment = Some(value);
        self
    }

    /// Set the publisher_user_id field (chainable)
    pub fn with_publisher_user_id(mut self, value: Uuid) -> Self {
        self.publisher_user_id = Some(value);
        self
    }

    /// Set the publisher_replied_at field (chainable)
    pub fn with_publisher_replied_at(mut self, value: DateTime<Utc>) -> Self {
        self.publisher_replied_at = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "rated_model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rated_model = v; }
                }
                "rated_record_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rated_record_id = v; }
                }
                "parent_rated_model" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parent_rated_model = v; }
                }
                "parent_rated_record_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.parent_rated_record_id = v; }
                }
                "rated_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rated_user_id = v; }
                }
                "rater_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rater_user_id = v; }
                }
                "rater_email" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rater_email = v; }
                }
                "rating_value" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rating_value = v; }
                }
                "feedback" => {
                    if let Ok(v) = serde_json::from_value(value) { self.feedback = v; }
                }
                "consumed" => {
                    if let Ok(v) = serde_json::from_value(value) { self.consumed = v; }
                }
                "rated_on" => {
                    if let Ok(v) = serde_json::from_value(value) { self.rated_on = v; }
                }
                "token_nonce" => {
                    if let Ok(v) = serde_json::from_value(value) { self.token_nonce = v; }
                }
                "token_expires_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.token_expires_at = v; }
                }
                "publisher_comment" => {
                    if let Ok(v) = serde_json::from_value(value) { self.publisher_comment = v; }
                }
                "publisher_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.publisher_user_id = v; }
                }
                "publisher_replied_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.publisher_replied_at = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for EngagementRating {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "EngagementRating"
    }
}

impl backbone_core::PersistentEntity for EngagementRating {
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

impl backbone_orm::EntityRepoMeta for EngagementRating {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("rated_record_id".to_string(), "uuid".to_string());
        m.insert("parent_rated_record_id".to_string(), "uuid".to_string());
        m.insert("rated_user_id".to_string(), "uuid".to_string());
        m.insert("rater_user_id".to_string(), "uuid".to_string());
        m.insert("publisher_user_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["rated_model", "token_nonce"]
    }
}

/// Builder for EngagementRating entity
///
/// Provides a fluent API for constructing EngagementRating instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct EngagementRatingBuilder {
    rated_model: Option<String>,
    rated_record_id: Option<Uuid>,
    parent_rated_model: Option<String>,
    parent_rated_record_id: Option<Uuid>,
    rated_user_id: Option<Uuid>,
    rater_user_id: Option<Uuid>,
    rater_email: Option<String>,
    rating_value: Option<i32>,
    feedback: Option<String>,
    consumed: Option<bool>,
    rated_on: Option<DateTime<Utc>>,
    token_nonce: Option<String>,
    token_expires_at: Option<DateTime<Utc>>,
    publisher_comment: Option<String>,
    publisher_user_id: Option<Uuid>,
    publisher_replied_at: Option<DateTime<Utc>>,
}

impl EngagementRatingBuilder {
    /// Set the rated_model field (required)
    pub fn rated_model(mut self, value: String) -> Self {
        self.rated_model = Some(value);
        self
    }

    /// Set the rated_record_id field (required)
    pub fn rated_record_id(mut self, value: Uuid) -> Self {
        self.rated_record_id = Some(value);
        self
    }

    /// Set the parent_rated_model field (optional)
    pub fn parent_rated_model(mut self, value: String) -> Self {
        self.parent_rated_model = Some(value);
        self
    }

    /// Set the parent_rated_record_id field (optional)
    pub fn parent_rated_record_id(mut self, value: Uuid) -> Self {
        self.parent_rated_record_id = Some(value);
        self
    }

    /// Set the rated_user_id field (optional)
    pub fn rated_user_id(mut self, value: Uuid) -> Self {
        self.rated_user_id = Some(value);
        self
    }

    /// Set the rater_user_id field (optional)
    pub fn rater_user_id(mut self, value: Uuid) -> Self {
        self.rater_user_id = Some(value);
        self
    }

    /// Set the rater_email field (optional)
    pub fn rater_email(mut self, value: String) -> Self {
        self.rater_email = Some(value);
        self
    }

    /// Set the rating_value field (optional)
    pub fn rating_value(mut self, value: i32) -> Self {
        self.rating_value = Some(value);
        self
    }

    /// Set the feedback field (optional)
    pub fn feedback(mut self, value: String) -> Self {
        self.feedback = Some(value);
        self
    }

    /// Set the consumed field (default: `false`)
    pub fn consumed(mut self, value: bool) -> Self {
        self.consumed = Some(value);
        self
    }

    /// Set the rated_on field (optional)
    pub fn rated_on(mut self, value: DateTime<Utc>) -> Self {
        self.rated_on = Some(value);
        self
    }

    /// Set the token_nonce field (required)
    pub fn token_nonce(mut self, value: String) -> Self {
        self.token_nonce = Some(value);
        self
    }

    /// Set the token_expires_at field (required)
    pub fn token_expires_at(mut self, value: DateTime<Utc>) -> Self {
        self.token_expires_at = Some(value);
        self
    }

    /// Set the publisher_comment field (optional)
    pub fn publisher_comment(mut self, value: String) -> Self {
        self.publisher_comment = Some(value);
        self
    }

    /// Set the publisher_user_id field (optional)
    pub fn publisher_user_id(mut self, value: Uuid) -> Self {
        self.publisher_user_id = Some(value);
        self
    }

    /// Set the publisher_replied_at field (optional)
    pub fn publisher_replied_at(mut self, value: DateTime<Utc>) -> Self {
        self.publisher_replied_at = Some(value);
        self
    }

    /// Build the EngagementRating entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<EngagementRating, String> {
        let rated_model = self.rated_model.ok_or_else(|| "rated_model is required".to_string())?;
        let rated_record_id = self.rated_record_id.ok_or_else(|| "rated_record_id is required".to_string())?;
        let token_nonce = self.token_nonce.ok_or_else(|| "token_nonce is required".to_string())?;
        let token_expires_at = self.token_expires_at.ok_or_else(|| "token_expires_at is required".to_string())?;

        Ok(EngagementRating {
            id: Uuid::new_v4(),
            rated_model,
            rated_record_id,
            parent_rated_model: self.parent_rated_model,
            parent_rated_record_id: self.parent_rated_record_id,
            rated_user_id: self.rated_user_id,
            rater_user_id: self.rater_user_id,
            rater_email: self.rater_email,
            rating_value: self.rating_value,
            feedback: self.feedback,
            consumed: self.consumed.unwrap_or(false),
            rated_on: self.rated_on,
            token_nonce,
            token_expires_at,
            publisher_comment: self.publisher_comment,
            publisher_user_id: self.publisher_user_id,
            publisher_replied_at: self.publisher_replied_at,
            metadata: AuditMetadata::default(),
        })
    }
}
