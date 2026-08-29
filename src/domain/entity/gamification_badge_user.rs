use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::BadgeGrantKind;
use super::BadgeLevel;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationBadgeUser
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationBadgeUserId(pub Uuid);

impl GamificationBadgeUserId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationBadgeUserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationBadgeUserId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationBadgeUserId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationBadgeUserId> for Uuid {
    fn from(id: GamificationBadgeUserId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationBadgeUserId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationBadgeUserId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationBadgeUser {
    pub id: Uuid,
    pub badge_id: Uuid,
    pub recipient_user_id: Uuid,
    pub sender_user_id: Option<Uuid>,
    pub grant_kind: BadgeGrantKind,
    pub challenge_id: Option<Uuid>,
    pub comment: Option<String>,
    pub level: Option<BadgeLevel>,
    pub granted_at: DateTime<Utc>,
    pub grant_key: Option<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationBadgeUser {
    /// Create a builder for GamificationBadgeUser
    pub fn builder() -> GamificationBadgeUserBuilder {
        <GamificationBadgeUserBuilder as Default>::default()
    }

    /// Create a new GamificationBadgeUser with required fields
    pub fn new(badge_id: Uuid, recipient_user_id: Uuid, grant_kind: BadgeGrantKind, granted_at: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            badge_id,
            recipient_user_id,
            sender_user_id: None,
            grant_kind,
            challenge_id: None,
            comment: None,
            level: None,
            granted_at,
            grant_key: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationBadgeUserId {
        GamificationBadgeUserId(self.id)
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

    /// Set the sender_user_id field (chainable)
    pub fn with_sender_user_id(mut self, value: Uuid) -> Self {
        self.sender_user_id = Some(value);
        self
    }

    /// Set the challenge_id field (chainable)
    pub fn with_challenge_id(mut self, value: Uuid) -> Self {
        self.challenge_id = Some(value);
        self
    }

    /// Set the comment field (chainable)
    pub fn with_comment(mut self, value: String) -> Self {
        self.comment = Some(value);
        self
    }

    /// Set the level field (chainable)
    pub fn with_level(mut self, value: BadgeLevel) -> Self {
        self.level = Some(value);
        self
    }

    /// Set the grant_key field (chainable)
    pub fn with_grant_key(mut self, value: String) -> Self {
        self.grant_key = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "badge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.badge_id = v; }
                }
                "recipient_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.recipient_user_id = v; }
                }
                "sender_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sender_user_id = v; }
                }
                "grant_kind" => {
                    if let Ok(v) = serde_json::from_value(value) { self.grant_kind = v; }
                }
                "challenge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.challenge_id = v; }
                }
                "comment" => {
                    if let Ok(v) = serde_json::from_value(value) { self.comment = v; }
                }
                "level" => {
                    if let Ok(v) = serde_json::from_value(value) { self.level = v; }
                }
                "granted_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.granted_at = v; }
                }
                "grant_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.grant_key = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationBadgeUser {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationBadgeUser"
    }
}

impl backbone_core::PersistentEntity for GamificationBadgeUser {
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

impl backbone_orm::EntityRepoMeta for GamificationBadgeUser {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("badge_id".to_string(), "uuid".to_string());
        m.insert("recipient_user_id".to_string(), "uuid".to_string());
        m.insert("sender_user_id".to_string(), "uuid".to_string());
        m.insert("challenge_id".to_string(), "uuid".to_string());
        m.insert("grant_kind".to_string(), "badge_grant_kind".to_string());
        m.insert("level".to_string(), "badge_level".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for GamificationBadgeUser entity
///
/// Provides a fluent API for constructing GamificationBadgeUser instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationBadgeUserBuilder {
    badge_id: Option<Uuid>,
    recipient_user_id: Option<Uuid>,
    sender_user_id: Option<Uuid>,
    grant_kind: Option<BadgeGrantKind>,
    challenge_id: Option<Uuid>,
    comment: Option<String>,
    level: Option<BadgeLevel>,
    granted_at: Option<DateTime<Utc>>,
    grant_key: Option<String>,
}

impl GamificationBadgeUserBuilder {
    /// Set the badge_id field (required)
    pub fn badge_id(mut self, value: Uuid) -> Self {
        self.badge_id = Some(value);
        self
    }

    /// Set the recipient_user_id field (required)
    pub fn recipient_user_id(mut self, value: Uuid) -> Self {
        self.recipient_user_id = Some(value);
        self
    }

    /// Set the sender_user_id field (optional)
    pub fn sender_user_id(mut self, value: Uuid) -> Self {
        self.sender_user_id = Some(value);
        self
    }

    /// Set the grant_kind field (default: `BadgeGrantKind::default()`)
    pub fn grant_kind(mut self, value: BadgeGrantKind) -> Self {
        self.grant_kind = Some(value);
        self
    }

    /// Set the challenge_id field (optional)
    pub fn challenge_id(mut self, value: Uuid) -> Self {
        self.challenge_id = Some(value);
        self
    }

    /// Set the comment field (optional)
    pub fn comment(mut self, value: String) -> Self {
        self.comment = Some(value);
        self
    }

    /// Set the level field (optional)
    pub fn level(mut self, value: BadgeLevel) -> Self {
        self.level = Some(value);
        self
    }

    /// Set the granted_at field (default: `Utc::now()`)
    pub fn granted_at(mut self, value: DateTime<Utc>) -> Self {
        self.granted_at = Some(value);
        self
    }

    /// Set the grant_key field (optional)
    pub fn grant_key(mut self, value: String) -> Self {
        self.grant_key = Some(value);
        self
    }

    /// Build the GamificationBadgeUser entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationBadgeUser, String> {
        let badge_id = self.badge_id.ok_or_else(|| "badge_id is required".to_string())?;
        let recipient_user_id = self.recipient_user_id.ok_or_else(|| "recipient_user_id is required".to_string())?;

        Ok(GamificationBadgeUser {
            id: Uuid::new_v4(),
            badge_id,
            recipient_user_id,
            sender_user_id: self.sender_user_id,
            grant_kind: self.grant_kind.unwrap_or_default(),
            challenge_id: self.challenge_id,
            comment: self.comment,
            level: self.level,
            granted_at: self.granted_at.unwrap_or(Utc::now()),
            grant_key: self.grant_key,
            metadata: AuditMetadata::default(),
        })
    }
}
