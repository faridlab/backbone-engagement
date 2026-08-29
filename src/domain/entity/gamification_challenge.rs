use chrono::{DateTime, Utc, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::ChallengeState;
use super::ChallengePeriod;
use super::ChallengeVisibilityMode;
use super::ReportFrequency;
use super::AuditMetadata;

use crate::domain::state_machine::{challenge_stateStateMachine, challenge_stateState, StateMachineError};

/// Strongly-typed ID for GamificationChallenge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct GamificationChallengeId(pub Uuid);

impl GamificationChallengeId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for GamificationChallengeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for GamificationChallengeId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for GamificationChallengeId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<GamificationChallengeId> for Uuid {
    fn from(id: GamificationChallengeId) -> Self { id.0 }
}

impl AsRef<Uuid> for GamificationChallengeId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for GamificationChallengeId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct GamificationChallenge {
    pub id: Uuid,
    pub name: String,
    pub(crate) state: ChallengeState,
    pub manager_user_id: Option<Uuid>,
    pub period: ChallengePeriod,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub visibility_mode: ChallengeVisibilityMode,
    pub reward_badge_id: Option<Uuid>,
    pub reward_first_badge_id: Option<Uuid>,
    pub reward_second_badge_id: Option<Uuid>,
    pub reward_third_badge_id: Option<Uuid>,
    pub reward_failure: bool,
    pub reward_realtime: bool,
    pub report_frequency: ReportFrequency,
    pub last_report_date: Option<NaiveDate>,
    pub next_report_date: Option<NaiveDate>,
    pub remind_update_delay: Option<i32>,
    pub user_ids: serde_json::Value,
    pub include_all_users: bool,
    pub include_badge_ids: serde_json::Value,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl GamificationChallenge {
    /// Create a builder for GamificationChallenge
    pub fn builder() -> GamificationChallengeBuilder {
        <GamificationChallengeBuilder as Default>::default()
    }

    /// Create a new GamificationChallenge with required fields
    pub fn new(name: String, state: ChallengeState, period: ChallengePeriod, visibility_mode: ChallengeVisibilityMode, reward_failure: bool, reward_realtime: bool, report_frequency: ReportFrequency, user_ids: serde_json::Value, include_all_users: bool, include_badge_ids: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            state,
            manager_user_id: None,
            period,
            start_date: None,
            end_date: None,
            visibility_mode,
            reward_badge_id: None,
            reward_first_badge_id: None,
            reward_second_badge_id: None,
            reward_third_badge_id: None,
            reward_failure,
            reward_realtime,
            report_frequency,
            last_report_date: None,
            next_report_date: None,
            remind_update_delay: None,
            user_ids,
            include_all_users,
            include_badge_ids,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> GamificationChallengeId {
        GamificationChallengeId(self.id)
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

    /// Set the manager_user_id field (chainable)
    pub fn with_manager_user_id(mut self, value: Uuid) -> Self {
        self.manager_user_id = Some(value);
        self
    }

    /// Set the start_date field (chainable)
    pub fn with_start_date(mut self, value: NaiveDate) -> Self {
        self.start_date = Some(value);
        self
    }

    /// Set the end_date field (chainable)
    pub fn with_end_date(mut self, value: NaiveDate) -> Self {
        self.end_date = Some(value);
        self
    }

    /// Set the reward_badge_id field (chainable)
    pub fn with_reward_badge_id(mut self, value: Uuid) -> Self {
        self.reward_badge_id = Some(value);
        self
    }

    /// Set the reward_first_badge_id field (chainable)
    pub fn with_reward_first_badge_id(mut self, value: Uuid) -> Self {
        self.reward_first_badge_id = Some(value);
        self
    }

    /// Set the reward_second_badge_id field (chainable)
    pub fn with_reward_second_badge_id(mut self, value: Uuid) -> Self {
        self.reward_second_badge_id = Some(value);
        self
    }

    /// Set the reward_third_badge_id field (chainable)
    pub fn with_reward_third_badge_id(mut self, value: Uuid) -> Self {
        self.reward_third_badge_id = Some(value);
        self
    }

    /// Set the last_report_date field (chainable)
    pub fn with_last_report_date(mut self, value: NaiveDate) -> Self {
        self.last_report_date = Some(value);
        self
    }

    /// Set the next_report_date field (chainable)
    pub fn with_next_report_date(mut self, value: NaiveDate) -> Self {
        self.next_report_date = Some(value);
        self
    }

    /// Set the remind_update_delay field (chainable)
    pub fn with_remind_update_delay(mut self, value: i32) -> Self {
        self.remind_update_delay = Some(value);
        self
    }

    // ==========================================================
    // State Machine
    // ==========================================================

    /// Transition to a new state via the state state machine.
    ///
    /// Returns `Err` if the transition is not permitted from the current state.
    /// Use this method instead of assigning `self.state` directly.
    pub fn transition_to(&mut self, new_state: challenge_stateState) -> Result<(), StateMachineError> {
        let current = self.state.to_string().parse::<challenge_stateState>()?;
        let mut sm = challenge_stateStateMachine::from_state(current);
        sm.transition_to_state(new_state)?;
        self.state = new_state.to_string().parse::<ChallengeState>()
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
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "manager_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.manager_user_id = v; }
                }
                "period" => {
                    if let Ok(v) = serde_json::from_value(value) { self.period = v; }
                }
                "start_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.start_date = v; }
                }
                "end_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.end_date = v; }
                }
                "visibility_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.visibility_mode = v; }
                }
                "reward_badge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_badge_id = v; }
                }
                "reward_first_badge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_first_badge_id = v; }
                }
                "reward_second_badge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_second_badge_id = v; }
                }
                "reward_third_badge_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_third_badge_id = v; }
                }
                "reward_failure" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_failure = v; }
                }
                "reward_realtime" => {
                    if let Ok(v) = serde_json::from_value(value) { self.reward_realtime = v; }
                }
                "report_frequency" => {
                    if let Ok(v) = serde_json::from_value(value) { self.report_frequency = v; }
                }
                "last_report_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_report_date = v; }
                }
                "next_report_date" => {
                    if let Ok(v) = serde_json::from_value(value) { self.next_report_date = v; }
                }
                "remind_update_delay" => {
                    if let Ok(v) = serde_json::from_value(value) { self.remind_update_delay = v; }
                }
                "user_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_ids = v; }
                }
                "include_all_users" => {
                    if let Ok(v) = serde_json::from_value(value) { self.include_all_users = v; }
                }
                "include_badge_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.include_badge_ids = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for GamificationChallenge {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "GamificationChallenge"
    }
}

impl backbone_core::PersistentEntity for GamificationChallenge {
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

impl backbone_orm::EntityRepoMeta for GamificationChallenge {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("manager_user_id".to_string(), "uuid".to_string());
        m.insert("reward_badge_id".to_string(), "uuid".to_string());
        m.insert("reward_first_badge_id".to_string(), "uuid".to_string());
        m.insert("reward_second_badge_id".to_string(), "uuid".to_string());
        m.insert("reward_third_badge_id".to_string(), "uuid".to_string());
        m.insert("state".to_string(), "challenge_state".to_string());
        m.insert("period".to_string(), "challenge_period".to_string());
        m.insert("visibility_mode".to_string(), "challenge_visibility_mode".to_string());
        m.insert("report_frequency".to_string(), "report_frequency".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for GamificationChallenge entity
///
/// Provides a fluent API for constructing GamificationChallenge instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct GamificationChallengeBuilder {
    name: Option<String>,
    state: Option<ChallengeState>,
    manager_user_id: Option<Uuid>,
    period: Option<ChallengePeriod>,
    start_date: Option<NaiveDate>,
    end_date: Option<NaiveDate>,
    visibility_mode: Option<ChallengeVisibilityMode>,
    reward_badge_id: Option<Uuid>,
    reward_first_badge_id: Option<Uuid>,
    reward_second_badge_id: Option<Uuid>,
    reward_third_badge_id: Option<Uuid>,
    reward_failure: Option<bool>,
    reward_realtime: Option<bool>,
    report_frequency: Option<ReportFrequency>,
    last_report_date: Option<NaiveDate>,
    next_report_date: Option<NaiveDate>,
    remind_update_delay: Option<i32>,
    user_ids: Option<serde_json::Value>,
    include_all_users: Option<bool>,
    include_badge_ids: Option<serde_json::Value>,
}

impl GamificationChallengeBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the state field (default: `ChallengeState::default()`)
    pub fn state(mut self, value: ChallengeState) -> Self {
        self.state = Some(value);
        self
    }

    /// Set the manager_user_id field (optional)
    pub fn manager_user_id(mut self, value: Uuid) -> Self {
        self.manager_user_id = Some(value);
        self
    }

    /// Set the period field (default: `ChallengePeriod::default()`)
    pub fn period(mut self, value: ChallengePeriod) -> Self {
        self.period = Some(value);
        self
    }

    /// Set the start_date field (optional)
    pub fn start_date(mut self, value: NaiveDate) -> Self {
        self.start_date = Some(value);
        self
    }

    /// Set the end_date field (optional)
    pub fn end_date(mut self, value: NaiveDate) -> Self {
        self.end_date = Some(value);
        self
    }

    /// Set the visibility_mode field (default: `ChallengeVisibilityMode::default()`)
    pub fn visibility_mode(mut self, value: ChallengeVisibilityMode) -> Self {
        self.visibility_mode = Some(value);
        self
    }

    /// Set the reward_badge_id field (optional)
    pub fn reward_badge_id(mut self, value: Uuid) -> Self {
        self.reward_badge_id = Some(value);
        self
    }

    /// Set the reward_first_badge_id field (optional)
    pub fn reward_first_badge_id(mut self, value: Uuid) -> Self {
        self.reward_first_badge_id = Some(value);
        self
    }

    /// Set the reward_second_badge_id field (optional)
    pub fn reward_second_badge_id(mut self, value: Uuid) -> Self {
        self.reward_second_badge_id = Some(value);
        self
    }

    /// Set the reward_third_badge_id field (optional)
    pub fn reward_third_badge_id(mut self, value: Uuid) -> Self {
        self.reward_third_badge_id = Some(value);
        self
    }

    /// Set the reward_failure field (default: `false`)
    pub fn reward_failure(mut self, value: bool) -> Self {
        self.reward_failure = Some(value);
        self
    }

    /// Set the reward_realtime field (default: `true`)
    pub fn reward_realtime(mut self, value: bool) -> Self {
        self.reward_realtime = Some(value);
        self
    }

    /// Set the report_frequency field (default: `ReportFrequency::default()`)
    pub fn report_frequency(mut self, value: ReportFrequency) -> Self {
        self.report_frequency = Some(value);
        self
    }

    /// Set the last_report_date field (optional)
    pub fn last_report_date(mut self, value: NaiveDate) -> Self {
        self.last_report_date = Some(value);
        self
    }

    /// Set the next_report_date field (optional)
    pub fn next_report_date(mut self, value: NaiveDate) -> Self {
        self.next_report_date = Some(value);
        self
    }

    /// Set the remind_update_delay field (optional)
    pub fn remind_update_delay(mut self, value: i32) -> Self {
        self.remind_update_delay = Some(value);
        self
    }

    /// Set the user_ids field (default: `serde_json::json!([])`)
    pub fn user_ids(mut self, value: serde_json::Value) -> Self {
        self.user_ids = Some(value);
        self
    }

    /// Set the include_all_users field (default: `false`)
    pub fn include_all_users(mut self, value: bool) -> Self {
        self.include_all_users = Some(value);
        self
    }

    /// Set the include_badge_ids field (default: `serde_json::json!([])`)
    pub fn include_badge_ids(mut self, value: serde_json::Value) -> Self {
        self.include_badge_ids = Some(value);
        self
    }

    /// Build the GamificationChallenge entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<GamificationChallenge, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(GamificationChallenge {
            id: Uuid::new_v4(),
            name,
            state: self.state.unwrap_or_default(),
            manager_user_id: self.manager_user_id,
            period: self.period.unwrap_or_default(),
            start_date: self.start_date,
            end_date: self.end_date,
            visibility_mode: self.visibility_mode.unwrap_or_default(),
            reward_badge_id: self.reward_badge_id,
            reward_first_badge_id: self.reward_first_badge_id,
            reward_second_badge_id: self.reward_second_badge_id,
            reward_third_badge_id: self.reward_third_badge_id,
            reward_failure: self.reward_failure.unwrap_or(false),
            reward_realtime: self.reward_realtime.unwrap_or(true),
            report_frequency: self.report_frequency.unwrap_or_default(),
            last_report_date: self.last_report_date,
            next_report_date: self.next_report_date,
            remind_update_delay: self.remind_update_delay,
            user_ids: self.user_ids.unwrap_or(serde_json::json!([])),
            include_all_users: self.include_all_users.unwrap_or(false),
            include_badge_ids: self.include_badge_ids.unwrap_or(serde_json::json!([])),
            metadata: AuditMetadata::default(),
        })
    }
}
