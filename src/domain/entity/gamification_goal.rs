use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use rust_decimal::Decimal;

use super::GoalState;
use super::AuditMetadata;

use crate::domain::state_machine::{goal_stateStateMachine, goal_stateState, StateMachineError};

/// Strongly-typed ID for GamificationGoal
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationGoalId(pub Uuid);

impl GamificationGoalId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationGoalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationGoalId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationGoalId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationGoalId> for Uuid {
    fn from(id: GamificationGoalId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationGoalId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationGoalId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationGoal {
    pub id: Uuid,
    pub definition_id: Uuid,
    pub user_id: Uuid,
    pub line_id: Option<Uuid>,
    pub challenge_id: Option<Uuid>,
    pub start_date: NaiveDate,
    pub end_date: Option<NaiveDate>,
    pub target: Decimal,
    pub current: Decimal,
    pub(crate) state: GoalState,
    pub to_update: bool,
    pub last_update: Option<NaiveDate>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationGoal {
    /// Create a builder for GamificationGoal
    pub fn builder() -> GamificationGoalBuilder {
        <GamificationGoalBuilder as Default>::default()
    }

    /// Create a new GamificationGoal with required fields
    pub fn new(definition_id: Uuid, user_id: Uuid, start_date: NaiveDate, target: Decimal, current: Decimal, state: GoalState, to_update: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            definition_id,
            user_id,
            line_id: None,
            challenge_id: None,
            start_date,
            end_date: None,
            target,
            current,
            state,
            to_update,
            last_update: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationGoalId {
        GamificationGoalId(self.id)
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

    /// Set the line_id field (chainable)
    pub fn with_line_id(mut self, value: Uuid) -> Self {
        self.line_id = Some(value);
        self
    }

    /// Set the challenge_id field (chainable)
    pub fn with_challenge_id(mut self, value: Uuid) -> Self {
        self.challenge_id = Some(value);
        self
    }

    /// Set the end_date field (chainable)
    pub fn with_end_date(mut self, value: NaiveDate) -> Self {
        self.end_date = Some(value);
        self
    }

    /// Set the last_update field (chainable)
    pub fn with_last_update(mut self, value: NaiveDate) -> Self {
        self.last_update = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the state state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.state` directly.
    pub fn transition_to(&mut self, new_state: goal_stateState) -> Result<(), StateMachineError> {
        let current = self.state.to_string().parse::<goal_stateState>()?;
        let mut sm = goal_stateStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.state = new_state.to_string().parse::<GoalState>()
            .map_err(|e| StateMachineError::InvalidState(e.to_string()))?;
        Ok(())
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "definition_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.definition_id = v; }
                }
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "line_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.line_id = v; }
                }
                "challenge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.challenge_id = v; }
                }
                "start_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.start_date = v; }
                }
                "end_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.end_date = v; }
                }
                "target" => {
                    if let Ok(v) = serde_json::from_value(value) { self.target = v; }
                }
                "current" => {
                    if let Ok(v) = serde_json::from_value(value) { self.current = v; }
                }
                "to_update" => {
                    if let Ok(v) = serde_json::from_value(value) { self.to_update = v; }
                }
                "last_update" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_update = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationGoal {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationGoal"
    }
}

impl backbone_core::PersistentEntity for GamificationGoal {
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

impl backbone_orm::EntityRepoMeta for GamificationGoal {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("definition_id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m.insert("line_id".to_string(), "uuid".to_string());
        m.insert("challenge_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "goal_state".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for GamificationGoal entity
///
/// Provides a fluent API for constructing GamificationGoal instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationGoalBuilder {
    definition_id: Option<Uuid>,
    user_id: Option<Uuid>,
    line_id: Option<Uuid>,
    challenge_id: Option<Uuid>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    target: Option<Decimal>,
    current: Option<Decimal>,
    state: Option<GoalState>,
    to_update: Option<bool>,
    last_update: Option<NaiveDate>,
}

impl GamificationGoalBuilder {
    /// Set the definition_id field (required)
    pub fn definition_id(mut self, value: Uuid) -> Self {
        self.definition_id = Some(value);
        self
    }

    /// Set the user_id field (required)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the line_id field (optional)
    pub fn line_id(mut self, value: Uuid) -> Self {
        self.line_id = Some(value);
        self
    }

    /// Set the challenge_id field (optional)
    pub fn challenge_id(mut self, value: Uuid) -> Self {
        self.challenge_id = Some(value);
        self
    }

    /// Set the start_date field (required)
    pub fn start_date(mut self, value: NaiveDate) -> Self {
        self.start_date = Some(value);
        self
    }

    /// Set the end_date field (optional)
    pub fn end_date(mut self, value: NaiveDate) -> Self {
        self.end_date = Some(value);
        self
    }

    /// Set the target field (required)
    pub fn target(mut self, value: Decimal) -> Self {
        self.target = Some(value);
        self
    }

    /// Set the current field (default: `Decimal::from(0)`)
    pub fn current(mut self, value: Decimal) -> Self {
        self.current = Some(value);
        self
    }

    /// Set the state field (default: `GoalState::default()`)
    pub fn state(mut self, value: GoalState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the to_update field (default: `false`)
    pub fn to_update(mut self, value: bool) -> Self {
        self.to_update = Some(value);
        self
    }

    /// Set the last_update field (optional)
    pub fn last_update(mut self, value: NaiveDate) -> Self {
        self.last_update = Some(value);
        self
    }

    /// Build the GamificationGoal entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationGoal, String> {
        let definition_id = self.definition_id.ok_or_else(|| "definition_id is required".to_string())?;
        let user_id = self.user_id.ok_or_else(|| "user_id is required".to_string())?;
        let start_date = self.start_date.ok_or_else(|| "start_date is required".to_string())?;
        let target = self.target.ok_or_else(|| "target is required".to_string())?;

        Ok(GamificationGoal {
            id: Uuid::new_v4(),
            definition_id,
            user_id,
            line_id: self.line_id,
            challenge_id: self.challenge_id,
            start_date,
            end_date: self.end_date,
            target,
            current: self.current.unwrap_or(Decimal::from(0)),
            state: self.state.unwrap_or_default(),
            to_update: self.to_update.unwrap_or(false),
            last_update: self.last_update,
            metadata: AuditMetadata::default(),
        })
    }
}
