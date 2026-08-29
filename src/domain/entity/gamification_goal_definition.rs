use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::GoalComputationMode;
use super::GoalCondition;
use super::GoalDisplayMode;
use super::AuditMetadata;

/// Strongly-typed ID for GamificationGoalDefinition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationGoalDefinitionId(pub Uuid);

impl GamificationGoalDefinitionId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationGoalDefinitionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationGoalDefinitionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationGoalDefinitionId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationGoalDefinitionId> for Uuid {
    fn from(id: GamificationGoalDefinitionId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationGoalDefinitionId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationGoalDefinitionId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationGoalDefinition {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub suffix: Option<String>,
    pub monetary: bool,
    pub computation_mode: GoalComputationMode,
    pub metric_key: Option<String>,
    pub condition: GoalCondition,
    pub display_mode: GoalDisplayMode,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationGoalDefinition {
    /// Create a builder for GamificationGoalDefinition
    pub fn builder() -> GamificationGoalDefinitionBuilder {
        <GamificationGoalDefinitionBuilder as Default>::default()
    }

    /// Create a new GamificationGoalDefinition with required fields
    pub fn new(name: String, monetary: bool, computation_mode: GoalComputationMode, condition: GoalCondition, display_mode: GoalDisplayMode) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            suffix: None,
            monetary,
            computation_mode,
            metric_key: None,
            condition,
            display_mode,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationGoalDefinitionId {
        GamificationGoalDefinitionId(self.id)
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

    /// Set the suffix field (chainable)
    pub fn with_suffix(mut self, value: String) -> Self {
        self.suffix = Some(value);
        self
    }

    /// Set the metric_key field (chainable)
    pub fn with_metric_key(mut self, value: String) -> Self {
        self.metric_key = Some(value);
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
                "description" => {
                    if let Ok(v) = serde_json::from_value(value) { self.description = v; }
                }
                "suffix" => {
                    if let Ok(v) = serde_json::from_value(value) { self.suffix = v; }
                }
                "monetary" => {
                    if let Ok(v) = serde_json::from_value(value) { self.monetary = v; }
                }
                "computation_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.computation_mode = v; }
                }
                "metric_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.metric_key = v; }
                }
                "condition" => {
                    if let Ok(v) = serde_json::from_value(value) { self.condition = v; }
                }
                "display_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.display_mode = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationGoalDefinition {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationGoalDefinition"
    }
}

impl backbone_core::PersistentEntity for GamificationGoalDefinition {
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

impl backbone_orm::EntityRepoMeta for GamificationGoalDefinition {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("computation_mode".to_string(), "goal_computation_mode".to_string());
        m.insert("condition".to_string(), "goal_condition".to_string());
        m.insert("display_mode".to_string(), "goal_display_mode".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for GamificationGoalDefinition entity
///
/// Provides a fluent API for constructing GamificationGoalDefinition instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationGoalDefinitionBuilder {
    name: Option<String>,
    description: Option<String>,
    suffix: Option<String>,
    monetary: Option<bool>,
    computation_mode: Option<GoalComputationMode>,
    metric_key: Option<String>,
    condition: Option<GoalCondition>,
    display_mode: Option<GoalDisplayMode>,
}

impl GamificationGoalDefinitionBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the description field (optional)
    pub fn description(mut self, value: String) -> Self {
        self.description = Some(value);
        self
    }

    /// Set the suffix field (optional)
    pub fn suffix(mut self, value: String) -> Self {
        self.suffix = Some(value);
        self
    }

    /// Set the monetary field (default: `false`)
    pub fn monetary(mut self, value: bool) -> Self {
        self.monetary = Some(value);
        self
    }

    /// Set the computation_mode field (default: `GoalComputationMode::default()`)
    pub fn computation_mode(mut self, value: GoalComputationMode) -> Self {
        self.computation_mode = Some(value);
        self
    }

    /// Set the metric_key field (optional)
    pub fn metric_key(mut self, value: String) -> Self {
        self.metric_key = Some(value);
        self
    }

    /// Set the condition field (default: `GoalCondition::default()`)
    pub fn condition(mut self, value: GoalCondition) -> Self {
        self.condition = Some(value);
        self
    }

    /// Set the display_mode field (default: `GoalDisplayMode::default()`)
    pub fn display_mode(mut self, value: GoalDisplayMode) -> Self {
        self.display_mode = Some(value);
        self
    }

    /// Build the GamificationGoalDefinition entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationGoalDefinition, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(GamificationGoalDefinition {
            id: Uuid::new_v4(),
            name,
            description: self.description,
            suffix: self.suffix,
            monetary: self.monetary.unwrap_or(false),
            computation_mode: self.computation_mode.unwrap_or_default(),
            metric_key: self.metric_key,
            condition: self.condition.unwrap_or_default(),
            display_mode: self.display_mode.unwrap_or_default(),
            metadata: AuditMetadata::default(),
        })
    }
}
