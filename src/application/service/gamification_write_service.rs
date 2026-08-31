//! `GamificationWriteService` — the validated gamification write path
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Family shape: a concrete service over the SQL held in
//! [`crate::infrastructure::persistence::gamification_repository`]
//! (services orchestrate, repositories hold SQL), an error enum carrying
//! `code()`/`http_status()`, and one transaction per verb.
//!
//! What lives here, and why:
//!
//! - **The karma ledger is append-only** — the append verb is its ONLY
//!   writer and the repository holds no UPDATE/DELETE SQL for it; a DB
//!   trigger raises on any other write (soft-deletes included). Balance is
//!   a PROJECTION (latest row per user), never a column; leaderboards rank
//!   by the SAME projection. Appends serialize per user on an advisory
//!   lock so `old_value` never goes stale. No consolidation exists —
//!   provenance is never rewritten.
//! - **Badge grants ride TWO verbs, no admin bypass.** `grant_peer` walks
//!   the `rule_auth` ladder (everyone/users/having/nobody) with the
//!   monthly cap and the self-grant refusal in the verb itself. The system
//!   verb `grant_system` is a DIFFERENT keyed verb (challenge/event
//!   kinds), not a bypassed ladder: a caller-supplied `grant_key` plus the
//!   partial UNIQUE index makes the grant exactly-once under at-least-once
//!   delivery — the survey certification contract's dedup. Badge level is
//!   snapshotted onto the grant row.
//! - **The certification inbound contract** (`certification_passed`, with
//!   the argument-shaped alias `on_certification_passed`) maps a typed
//!   `CertificationPassedFact` to `grant_system(kind = event)` with
//!   `grant_key = event:certification:{ref}:{attempt}` and NO challenge
//!   coupling — deleting challenges can never stop a certification grant.
//!   Payload shape is validated before any database work; a malformed fact
//!   is a typed refusal.
//! - **Declarative everything.** Membership is typed fields
//!   (`user_ids` + `include_all_users` + `include_badge_ids`) expanded at
//!   reconciliation — no stored domain text, no eval, ever. Goal
//!   definitions cite `metric_key` strings resolved against a
//!   composition-registered metric registry (code, not data); unknown keys
//!   are a typed 422 at write. The `include_all_users` expansion reads a
//!   composition-registered user directory (the user table lives in
//!   another module).
//! - **Both state machines move only through transition verbs** (ADR-0016
//!   hand_set). The daily check moves the SAME edges by dates THROUGH
//!   those verbs — never raw state writes. Goals generate at `current = 0`
//!   with an explicit first evaluation (never seeded dirty); the leaver
//!   sweep CANCELS goals; `manual_reach` re-computes the metric and
//!   refuses loudly when unmet; `done → draft` reset is refused while any
//!   live goal is inprogress.
//! - **Rewards dedupe through ONE mechanism** — the `grant_key` unique
//!   index. Realtime and podium grants share it, so a replay or a
//!   double-run converges.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::PgConnection;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entity::{
    BadgeGrantKind, BadgeLevel, BadgeRuleAuth, ChallengePeriod, ChallengeState, GamificationGoal,
    GamificationGoalDefinition, GoalComputationMode, GoalCondition, GoalState, ReportFrequency,
};
use crate::infrastructure::persistence::gamification_repository::GamificationRepository;

/// The registered origin vocabulary of the karma ledger (validated by the
/// append verb; extensible without schema change).
pub const ORIGIN_KINDS: &[&str] = &["manual", "badge", "challenge", "certification", "host"];

/// Longest accepted karma reason / grant comment.
pub const MAX_REASON_LEN: usize = 500;
/// Longest accepted grant key (the idempotency grain).
pub const MAX_GRANT_KEY_LEN: usize = 200;
/// Longest accepted metric key / definition name.
pub const MAX_KEY_LEN: usize = 200;
/// Default leaderboard size.
pub const DEFAULT_LEADERBOARD_LIMIT: i64 = 50;

// ─── error surface ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum GamificationError {
    #[error("badge not found")]
    BadgeNotFound,
    #[error("badge is archived and no longer grantable")]
    BadgeInactive,
    #[error("badge is not grantable by peers (ladder: {0})")]
    BadgeNotGrantable(&'static str),
    #[error("a sender cannot grant a badge to themselves")]
    BadgeSelfGrant,
    #[error("the monthly peer-grant cap for this badge is reached")]
    BadgeCapReached,
    #[error("no badge is registered under key: {0}")]
    BadgeKeyUnknown(String),
    #[error("certification payload failed validation: {0}")]
    CertificationPayloadInvalid(String),
    #[error("grant kind must be challenge or event")]
    GrantKindInvalid,
    #[error("grant key must be non-empty and at most {MAX_GRANT_KEY_LEN} characters")]
    GrantKeyInvalid,
    #[error("origin kind must be one of: {0}")]
    OriginKindUnknown(String),
    #[error("metric key is not registered: {0}")]
    MetricKeyUnknown(String),
    #[error("reason exceeds {MAX_REASON_LEN} characters")]
    ReasonTooLong,
    #[error("challenge not found")]
    ChallengeNotFound,
    #[error("challenge state does not allow this transition")]
    ChallengeInvalidState,
    #[error("challenge cannot be reset while a live goal is in progress")]
    ChallengeHasLiveGoals,
    #[error("include_all_users is set but no user directory is registered at composition")]
    DirectoryNotRegistered,
    #[error("goal not found")]
    GoalNotFound,
    #[error("goal state does not allow this transition")]
    GoalInvalidState,
    #[error("goal criteria not met — refusing to mark reached")]
    GoalCriteriaUnmet,
    #[error("goal definition not found")]
    DefinitionNotFound,
    #[error("only manual goals take human-entered values")]
    GoalNotManual,
    #[error("only registered-metric goals are evaluated by the check")]
    GoalNotComputed,
    #[error("internal error: {0}")]
    Internal(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl GamificationError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadgeNotFound => "badge_not_found",
            Self::BadgeInactive => "badge_inactive",
            Self::BadgeNotGrantable(_) => "badge_not_grantable",
            Self::BadgeSelfGrant => "badge_self_grant",
            Self::BadgeCapReached => "badge_cap_reached",
            Self::BadgeKeyUnknown(_) => "badge_key_unknown",
            Self::CertificationPayloadInvalid(_) => "certification_payload_invalid",
            Self::GrantKindInvalid => "grant_kind_invalid",
            Self::GrantKeyInvalid => "grant_key_invalid",
            Self::OriginKindUnknown(_) => "origin_kind_unknown",
            Self::MetricKeyUnknown(_) => "metric_key_unknown",
            Self::ReasonTooLong => "reason_too_long",
            Self::ChallengeNotFound => "challenge_not_found",
            Self::ChallengeInvalidState => "challenge_invalid_state",
            Self::ChallengeHasLiveGoals => "challenge_has_live_goals",
            Self::DirectoryNotRegistered => "directory_not_registered",
            Self::GoalNotFound => "goal_not_found",
            Self::GoalInvalidState => "goal_invalid_state",
            Self::GoalCriteriaUnmet => "goal_criteria_unmet",
            Self::DefinitionNotFound => "definition_not_found",
            Self::GoalNotManual => "goal_not_manual",
            Self::GoalNotComputed => "goal_not_computed",
            Self::Internal(_) => "internal_error",
            Self::Db(_) => "database_error",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::BadgeNotFound
            | Self::ChallengeNotFound
            | Self::GoalNotFound
            | Self::DefinitionNotFound => 404,
            Self::ChallengeInvalidState
            | Self::GoalInvalidState
            | Self::ChallengeHasLiveGoals => 409,
            Self::Internal(_) | Self::Db(_) | Self::DirectoryNotRegistered => 500,
            _ => 422,
        }
    }
}

// ─── the registries (code, not data) ──────────────────────────────────────────

/// A registered metric: computes a user's current value over a goal window.
/// Functions are code registered at composition — the ONLY computed mode
/// (every eval surface is banned).
pub type MetricFn =
    Arc<dyn Fn(Uuid, (NaiveDate, Option<NaiveDate>)) -> Decimal + Send + Sync>;

/// The name→compute-fn map goal definitions cite (the KPI-registry
/// precedent). Unknown keys are refused at definition write.
#[derive(Default)]
pub struct GamificationMetricRegistry {
    metrics: HashMap<String, MetricFn>,
}

impl GamificationMetricRegistry {
    pub fn new() -> Self {
        Self { metrics: HashMap::new() }
    }

    pub fn register(&mut self, key: &str, compute: MetricFn) {
        self.metrics.insert(key.to_string(), compute);
    }

    pub fn contains(&self, key: &str) -> bool {
        self.metrics.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<MetricFn> {
        self.metrics.get(key).cloned()
    }
}

/// Supplies the user population for `include_all_users` expansion (the
/// user table lives in another module — composition wires the listing).
pub type UserDirectory = Arc<dyn Fn() -> Vec<Uuid> + Send + Sync>;

// ─── inputs / outputs ─────────────────────────────────────────────────────────

/// One appended ledger row, with the rank-change fact (the host-relayed
/// `RankChanged` payload material).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KarmaAppendView {
    pub row_id: Uuid,
    pub user_id: Uuid,
    pub old_value: i32,
    pub new_value: i32,
    pub delta: i32,
    pub from_rank: Option<RankInfo>,
    pub to_rank: Option<RankInfo>,
}

/// A rank ladder rung (id + name snapshot).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RankInfo {
    pub id: Uuid,
    pub name: String,
    pub karma_min: i32,
}

/// One leaderboard row — the balance projection ranked.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaderboardEntry {
    pub position: i64,
    pub user_id: Uuid,
    pub balance: i32,
}

/// A minted (or deduped) grant row.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeGrantView {
    pub grant_row_id: Uuid,
    pub badge_id: Uuid,
    pub recipient_user_id: Uuid,
    pub grant_kind: BadgeGrantKind,
    pub grant_key: Option<String>,
    /// True when this call minted the row; false when the key had already
    /// granted (exactly-once convergence).
    pub created: bool,
}

/// The certification contract's result.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CertificationGrantView {
    pub grant_row_id: Uuid,
    pub badge_id: Uuid,
    pub recipient_user_id: Uuid,
    pub grant_key: String,
    pub created: bool,
}

/// One inbound `CertificationPassed` fact — the survey certification
/// contract's payload, field-for-field the event declaration in
/// `schema/hooks/index.hook.yaml`. The producing module (or a host relay
/// bridging processes) builds this struct and hands it to
/// [`GamificationWriteService::certification_passed`].
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CertificationPassedFact {
    /// The certification's own id, namespaced by the producer
    /// (a survey certificate carries `survey:{survey_id}`).
    pub certification_ref: String,
    /// The survey that issued the certification, when known. Carried for
    /// provenance; NOT stored on the grant row.
    pub survey_ref: Option<Uuid>,
    /// The winning attempt id — the idempotency input alongside
    /// `certification_ref`.
    pub attempt_ref: String,
    /// The certified user who earns the badge.
    pub recipient_user_id: Uuid,
    /// The badge's stable key — resolved to a `GamificationBadge` by name.
    pub badge_key: String,
}

impl CertificationPassedFact {
    /// The contract's idempotency grain: exactly one grant per
    /// (certification, attempt) pair, enforced by the `grant_key` partial
    /// UNIQUE index (rule R-G1). Under at-least-once delivery a redelivered
    /// fact converges on the first row instead of minting again.
    pub fn grant_key(&self) -> String {
        format!(
            "event:certification:{}:{}",
            self.certification_ref, self.attempt_ref
        )
    }

    /// Shape validation, BEFORE any database work: every reference is
    /// non-empty after trimming and bounded so the composed grant key can
    /// never overflow its index-bound length. Malformed facts are refused
    /// loudly — never silently granted, never silently dropped.
    pub fn validate(&self) -> Result<(), GamificationError> {
        let refuse = |detail: String| Err(GamificationError::CertificationPayloadInvalid(detail));
        for (name, value) in [
            ("certification_ref", &self.certification_ref),
            ("attempt_ref", &self.attempt_ref),
            ("badge_key", &self.badge_key),
        ] {
            if value.trim().is_empty() {
                return refuse(format!("{name} must not be empty"));
            }
            if value.chars().count() > MAX_KEY_LEN {
                return refuse(format!("{name} exceeds {MAX_KEY_LEN} characters"));
            }
        }
        if self.grant_key().chars().count() > MAX_GRANT_KEY_LEN {
            return refuse(format!(
                "composed grant key exceeds {MAX_GRANT_KEY_LEN} characters"
            ));
        }
        Ok(())
    }
}

/// A badge's grant stats (one grouped query).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BadgeStats {
    pub granted_count: i64,
    pub granted_users_count: i64,
    pub granted_this_month: i64,
}

/// A due report's computed payload (the host-relayed `ChallengeReportDue`
/// fact material — delivery is host composition).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChallengeReportDue {
    pub challenge_id: Uuid,
    pub mode: String,
    pub summary: serde_json::Value,
}

/// The daily check's outcome counters (one row per run; every effect is
/// idempotent, so a replayed run converges).
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCheckReport {
    pub challenges_processed: usize,
    pub challenges_started: usize,
    pub challenges_closed: usize,
    pub goals_generated: usize,
    pub goals_evaluated: usize,
    pub goals_canceled: usize,
    pub goals_failed: usize,
    pub rewards_granted: usize,
    pub rewards_deduped: usize,
    pub reports_due: Vec<ChallengeReportDue>,
}

// ─── helpers ──────────────────────────────────────────────────────────────────

/// Parse a json array of uuids (the no-SQL-FK house style for M2M-shaped
/// refs). Invalid entries are skipped, never fatal.
fn uuid_array(value: &serde_json::Value) -> Vec<Uuid> {
    value
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().and_then(|s| Uuid::parse_str(s).ok()))
                .collect()
        })
        .unwrap_or_default()
}

/// Read-side completeness derivation (never a column): `higher` is a
/// capped percentage toward the target; `lower` is binary 0-or-100.
fn completeness(condition: GoalCondition, current: Decimal, target: Decimal) -> Decimal {
    let hundred = Decimal::from(100);
    match condition {
        GoalCondition::Higher => {
            if target.is_zero() {
                Decimal::ZERO
            } else {
                (current / target).min(Decimal::ONE) * hundred
            }
        }
        GoalCondition::Lower => {
            if current <= target {
                hundred
            } else {
                Decimal::ZERO
            }
        }
    }
}

/// Is the goal's condition met at (current, target)?
fn condition_met(condition: GoalCondition, current: Decimal, target: Decimal) -> bool {
    match condition {
        GoalCondition::Higher => current >= target,
        GoalCondition::Lower => current <= target,
    }
}

/// The goal window for a period (the upstream `start_end_date_for_period`
/// semantics): `once` spans the challenge's own dates; recurring periods
/// derive fresh calendar windows.
fn period_window(
    period: ChallengePeriod,
    today: NaiveDate,
    challenge_end: Option<NaiveDate>,
) -> (NaiveDate, Option<NaiveDate>) {
    match period {
        ChallengePeriod::Once => (today, challenge_end),
        ChallengePeriod::Daily => (today, Some(today + Duration::days(1))),
        ChallengePeriod::Weekly => {
            // Monday of the ISO week, seven-day window.
            let weekday = today.weekday().num_days_from_monday();
            let monday = today - Duration::days(weekday as i64);
            (monday, Some(monday + Duration::days(6)))
        }
        ChallengePeriod::Monthly => {
            let first = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
                .unwrap_or(today);
            let (ny, nm) = if today.month() == 12 {
                (today.year() + 1, 1)
            } else {
                (today.year(), today.month() + 1)
            };
            let last = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or(today)
                - Duration::days(1);
            (first, Some(last))
        }
        ChallengePeriod::Yearly => {
            let first = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
            let last = NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap_or(today);
            (first, Some(last))
        }
    }
}

/// Advance a date by one report-cadence unit.
fn advance_report_date(from: NaiveDate, frequency: ReportFrequency) -> Option<NaiveDate> {
    match frequency {
        ReportFrequency::Never => None,
        ReportFrequency::Daily => Some(from + Duration::days(1)),
        ReportFrequency::Weekly => Some(from + Duration::days(7)),
        ReportFrequency::Monthly => {
            let (ny, nm) = if from.month() == 12 {
                (from.year() + 1, 1)
            } else {
                (from.year(), from.month() + 1)
            };
            let next = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap_or(from);
            Some(next)
        }
        ReportFrequency::Yearly => NaiveDate::from_ymd_opt(from.year() + 1, 1, 1),
    }
}

fn level_str(level: Option<BadgeLevel>) -> Option<&'static str> {
    level.map(|l| match l {
        BadgeLevel::Bronze => "bronze",
        BadgeLevel::Silver => "silver",
        BadgeLevel::Gold => "gold",
    })
}

fn ladder_name(auth: BadgeRuleAuth) -> &'static str {
    match auth {
        BadgeRuleAuth::Everyone => "everyone",
        BadgeRuleAuth::Users => "users",
        BadgeRuleAuth::Having => "having",
        BadgeRuleAuth::Nobody => "nobody",
    }
}

// ─── the service ──────────────────────────────────────────────────────────────

pub struct GamificationWriteService {
    pool: PgPool,
    metrics: GamificationMetricRegistry,
    directory: Option<UserDirectory>,
}

impl GamificationWriteService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            metrics: GamificationMetricRegistry::new(),
            directory: None,
        }
    }

    /// Register a metric at composition time (builder-style, before the
    /// service is shared). Definitions cite the key; unknown keys refuse
    /// at write (rule R-G10).
    pub fn register_metric(&mut self, key: &str, compute: MetricFn) -> &mut Self {
        self.metrics.register(key, compute);
        self
    }

    /// Register the user directory for `include_all_users` expansion
    /// (builder-style). An unset directory fails loud, never silent.
    pub fn set_user_directory(&mut self, directory: UserDirectory) -> &mut Self {
        self.directory = Some(directory);
        self
    }

    fn metric(&self, key: &str) -> Result<MetricFn, GamificationError> {
        self.metrics
            .get(key)
            .ok_or_else(|| GamificationError::MetricKeyUnknown(key.to_string()))
    }

    // ── the karma ledger (append-only) ───────────────────────────────────────

    /// Append one balance movement. THE only writer of the ledger: reads
    /// the projection under a per-user advisory lock, inserts old/new, and
    /// reports the rank-change fact (balance crossed a threshold). There
    /// is no update verb, no delete verb, and no consolidation — history
    /// is never rewritten.
    pub async fn append_karma(
        &self,
        user_id: Uuid,
        delta: i32,
        reason: Option<&str>,
        origin_kind: &str,
        origin_id: Option<Uuid>,
        origin_label: Option<&str>,
    ) -> Result<KarmaAppendView, GamificationError> {
        if !ORIGIN_KINDS.contains(&origin_kind) {
            return Err(GamificationError::OriginKindUnknown(origin_kind.to_string()));
        }
        let reason = reason.unwrap_or("manual adjustment");
        if reason.chars().count() > MAX_REASON_LEN {
            return Err(GamificationError::ReasonTooLong);
        }

        let mut tx = self.pool.begin().await?;
        GamificationRepository::lock_user_ledger(&mut tx, user_id).await?;

        let old_value = GamificationRepository::balance(&mut tx, user_id)
            .await?
            .unwrap_or(0);
        let new_value = old_value + delta;
        let row_id = GamificationRepository::insert_ledger_row(
            &mut tx,
            Uuid::new_v4(),
            user_id,
            old_value,
            new_value,
            reason,
            origin_kind,
            origin_id,
            origin_label,
        )
        .await?;

        let from_rank = GamificationRepository::rank_for_balance(&mut tx, old_value)
            .await?
            .map(|r| RankInfo { id: r.id, name: r.name, karma_min: r.karma_min });
        let to_rank = GamificationRepository::rank_for_balance(&mut tx, new_value)
            .await?
            .map(|r| RankInfo { id: r.id, name: r.name, karma_min: r.karma_min });

        tx.commit().await?;
        Ok(KarmaAppendView {
            row_id,
            user_id,
            old_value,
            new_value,
            delta,
            from_rank,
            to_rank,
        })
    }

    /// A user's balance — the projection read (latest ledger row's
    /// `new_value`; zero when no row exists).
    pub async fn balance(&self, user_id: Uuid) -> Result<i32, GamificationError> {
        let mut conn = self.pool.acquire().await?;
        Ok(GamificationRepository::balance(&mut conn, user_id)
            .await?
            .unwrap_or(0))
    }

    /// The leaderboard: every user's balance projection ranked descending
    /// (ties broken by user id — deterministic). The upstream SUM-window
    /// leaderboard is deliberately NOT ported: one table, ONE semantics.
    pub async fn leaderboard(&self, limit: Option<i64>) -> Result<Vec<LeaderboardEntry>, GamificationError> {
        let mut conn = self.pool.acquire().await?;
        let rows = GamificationRepository::leaderboard(
            &mut conn,
            limit.unwrap_or(DEFAULT_LEADERBOARD_LIMIT),
        )
        .await?;
        Ok(rows
            .into_iter()
            .enumerate()
            .map(|(idx, (user_id, balance))| LeaderboardEntry {
                position: idx as i64 + 1,
                user_id,
                balance,
            })
            .collect())
    }

    /// The rank a balance falls into, plus the next rung above it (the
    /// `karma_min DESC` walk — the ONE read-side derivation).
    pub async fn rank_for_balance(
        &self,
        balance: i32,
    ) -> Result<(Option<RankInfo>, Option<RankInfo>), GamificationError> {
        let mut conn = self.pool.acquire().await?;
        let current = GamificationRepository::rank_for_balance(&mut conn, balance)
            .await?
            .map(|r| RankInfo { id: r.id, name: r.name, karma_min: r.karma_min });
        let next = GamificationRepository::next_rank(&mut conn, balance)
            .await?
            .map(|r| RankInfo { id: r.id, name: r.name, karma_min: r.karma_min });
        Ok((current, next))
    }

    // ── badges: the two grant verbs ──────────────────────────────────────────

    /// The peer-grant verb: the declarative ladder (everyone / users /
    /// having / nobody — NO admin bypass exists; system grants are a
    /// different keyed verb), the monthly cap, and the self-grant
    /// refusal, all in the verb. Badge level snapshots onto the grant row.
    pub async fn grant_peer(
        &self,
        badge_id: Uuid,
        sender_user_id: Uuid,
        recipient_user_id: Uuid,
        comment: Option<&str>,
    ) -> Result<BadgeGrantView, GamificationError> {
        if let Some(c) = comment {
            if c.trim().chars().count() > MAX_REASON_LEN {
                return Err(GamificationError::ReasonTooLong);
            }
        }
        let mut tx = self.pool.begin().await?;

        let badge = GamificationRepository::find_badge(&mut tx, badge_id)
            .await?
            .ok_or(GamificationError::BadgeNotFound)?;
        if !badge.active {
            return Err(GamificationError::BadgeInactive);
        }
        if sender_user_id == recipient_user_id {
            return Err(GamificationError::BadgeSelfGrant);
        }

        // The ladder (rule R-G4).
        match badge.rule_auth {
            BadgeRuleAuth::Everyone => {}
            BadgeRuleAuth::Users => {
                let allowlist = uuid_array(&badge.rule_auth_user_ids);
                if !allowlist.contains(&sender_user_id) {
                    return Err(GamificationError::BadgeNotGrantable(ladder_name(badge.rule_auth)));
                }
            }
            BadgeRuleAuth::Having => {
                let prereqs = uuid_array(&badge.rule_auth_badge_ids);
                if !prereqs.is_empty() {
                    let held = GamificationRepository::count_badges_held(
                        &mut tx,
                        sender_user_id,
                        &prereqs,
                    )
                    .await?;
                    if held != prereqs.len() as i64 {
                        return Err(GamificationError::BadgeNotGrantable(
                            ladder_name(badge.rule_auth),
                        ));
                    }
                }
            }
            BadgeRuleAuth::Nobody => {
                return Err(GamificationError::BadgeNotGrantable(ladder_name(badge.rule_auth)));
            }
        }

        // The monthly cap (rule R-G5) — one grouped query.
        if badge.rule_max {
            match badge.rule_max_number {
                Some(cap) => {
                    let spent = GamificationRepository::peer_grants_this_month(
                        &mut tx,
                        badge_id,
                        sender_user_id,
                    )
                    .await?;
                    if spent >= cap as i64 {
                        return Err(GamificationError::BadgeCapReached);
                    }
                }
                // A cap toggle without a cap value is a misconfigured badge —
                // fail closed.
                None => {
                    return Err(GamificationError::Internal(
                        "badge rule_max is set without rule_max_number".into(),
                    ))
                }
            }
        }

        let row = GamificationRepository::insert_grant(
            &mut tx,
            Uuid::new_v4(),
            badge_id,
            recipient_user_id,
            Some(sender_user_id),
            "peer",
            None,
            comment.map(str::trim).filter(|c| !c.is_empty()),
            level_str(badge.level),
            None, // peer grants carry NULL keys: repeatable under the cap
        )
        .await?
        .ok_or_else(|| GamificationError::Internal("peer grant refused by constraint".into()))?;

        tx.commit().await?;
        Ok(BadgeGrantView {
            grant_row_id: row.id,
            badge_id,
            recipient_user_id,
            grant_kind: row.grant_kind,
            grant_key: row.grant_key,
            created: true,
        })
    }

    /// The keyed system grant (challenge / event kinds) — exactly-once per
    /// `grant_key` under at-least-once delivery: the partial UNIQUE index
    /// makes a concurrent or replayed delivery converge on the first row
    /// instead of minting (rule R-G1). Not a ladder bypass — a different
    /// verb with its own discipline.
    pub async fn grant_system(
        &self,
        badge_id: Uuid,
        recipient_user_id: Uuid,
        grant_kind: BadgeGrantKind,
        grant_key: &str,
        challenge_id: Option<Uuid>,
        comment: Option<&str>,
    ) -> Result<BadgeGrantView, GamificationError> {
        let mut tx = self.pool.begin().await?;
        let view = self
            .grant_system_on(&mut tx, badge_id, recipient_user_id, grant_kind, grant_key, challenge_id, comment)
            .await?;
        tx.commit().await?;
        Ok(view)
    }

    /// The system grant on the CALLER's connection — the challenge reward
    /// arms run inside the daily check's transaction so a reward and the
    /// state move that earned it commit (or roll back) together.
    #[allow(clippy::too_many_arguments)]
    async fn grant_system_on(
        &self,
        conn: &mut PgConnection,
        badge_id: Uuid,
        recipient_user_id: Uuid,
        grant_kind: BadgeGrantKind,
        grant_key: &str,
        challenge_id: Option<Uuid>,
        comment: Option<&str>,
    ) -> Result<BadgeGrantView, GamificationError> {
        if !matches!(grant_kind, BadgeGrantKind::Challenge | BadgeGrantKind::Event) {
            return Err(GamificationError::GrantKindInvalid);
        }
        let key = grant_key.trim();
        if key.is_empty() || key.chars().count() > MAX_GRANT_KEY_LEN {
            return Err(GamificationError::GrantKeyInvalid);
        }
        let kind_str = match grant_kind {
            BadgeGrantKind::Challenge => "challenge",
            _ => "event",
        };

        // System grants do not require the badge to be active (challenge
        // rewards reference live badges; archiving stops nothing silently).
        let badge = GamificationRepository::find_badge(conn, badge_id)
            .await?
            .ok_or(GamificationError::BadgeNotFound)?;

        let inserted = GamificationRepository::insert_grant(
            conn,
            Uuid::new_v4(),
            badge_id,
            recipient_user_id,
            None,
            kind_str,
            challenge_id,
            comment.map(str::trim).filter(|c| !c.is_empty()),
            level_str(badge.level),
            Some(key),
        )
        .await?;

        match inserted {
            Some(row) => Ok(BadgeGrantView {
                grant_row_id: row.id,
                badge_id,
                recipient_user_id,
                grant_kind: row.grant_kind,
                grant_key: row.grant_key,
                created: true,
            }),
            None => {
                // A concurrent delivery (or a replay) won: converge on the
                // existing row. Exactly-once holds.
                let existing = GamificationRepository::find_grant_by_key(conn, key)
                    .await?
                    .ok_or_else(|| {
                        GamificationError::Internal(
                            "grant-key conflict with no live row to converge on".into(),
                        )
                    })?;
                Ok(BadgeGrantView {
                    grant_row_id: existing.id,
                    badge_id: existing.badge_id,
                    recipient_user_id: existing.recipient_user_id,
                    grant_kind: existing.grant_kind,
                    grant_key: existing.grant_key,
                    created: false,
                })
            }
        }
    }

    /// THE inbound certification entry point (the survey module's
    /// completion fact): validate the payload shape, resolve the badge by
    /// its stable key, grant once per (certification, attempt) with NO
    /// challenge coupling — deleting challenges can never stop a
    /// certification grant. Callers that hold loose fields can use the
    /// argument-shaped [`Self::on_certification_passed`] alias.
    pub async fn certification_passed(
        &self,
        fact: &CertificationPassedFact,
    ) -> Result<CertificationGrantView, GamificationError> {
        fact.validate()?;
        let mut conn = self.pool.acquire().await?;
        let badge = GamificationRepository::find_badge_by_name(&mut conn, &fact.badge_key)
            .await?
            .ok_or_else(|| GamificationError::BadgeKeyUnknown(fact.badge_key.clone()))?;
        drop(conn);

        let grant_key = fact.grant_key();
        let view = self
            .grant_system(
                badge.id,
                fact.recipient_user_id,
                BadgeGrantKind::Event,
                &grant_key,
                None, // deliberately NULL: no challenge-coupled hidden stop
                None,
            )
            .await?;
        Ok(CertificationGrantView {
            grant_row_id: view.grant_row_id,
            badge_id: view.badge_id,
            recipient_user_id: view.recipient_user_id,
            grant_key,
            created: view.created,
        })
    }

    /// The argument-shaped alias of [`Self::certification_passed`] — the
    /// pinned name the survey contract's host adapter calls. Delegates to
    /// the typed verb, so validation and key derivation live in ONE place.
    pub async fn on_certification_passed(
        &self,
        certification_ref: &str,
        attempt_ref: &str,
        recipient_user_id: Uuid,
        badge_key: &str,
        survey_ref: Option<Uuid>,
    ) -> Result<CertificationGrantView, GamificationError> {
        self.certification_passed(&CertificationPassedFact {
            certification_ref: certification_ref.to_string(),
            survey_ref, // carried by the event contract; not stored on the grant
            attempt_ref: attempt_ref.to_string(),
            recipient_user_id,
            badge_key: badge_key.to_string(),
        })
        .await
    }

    /// A badge's grant stats (one grouped query).
    pub async fn badge_stats(&self, badge_id: Uuid) -> Result<BadgeStats, GamificationError> {
        let mut conn = self.pool.acquire().await?;
        let (granted_count, granted_users_count, granted_this_month) =
            GamificationRepository::badge_grant_stats(&mut conn, badge_id).await?;
        Ok(BadgeStats { granted_count, granted_users_count, granted_this_month })
    }

    // ── goal definitions ─────────────────────────────────────────────────────

    /// Create a goal definition through the typed verb: `metric_key` must
    /// exist in the registry when the mode is `registered_metric` (rule
    /// R-G10 — the write-time gate that replaces every eval surface).
    #[allow(clippy::too_many_arguments)]
    pub async fn create_goal_definition(
        &self,
        name: &str,
        description: Option<&str>,
        suffix: Option<&str>,
        monetary: bool,
        computation_mode: GoalComputationMode,
        metric_key: Option<&str>,
        condition: GoalCondition,
        display_mode: crate::domain::entity::GoalDisplayMode,
    ) -> Result<Uuid, GamificationError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > MAX_KEY_LEN {
            return Err(GamificationError::MetricKeyUnknown(name.to_string()));
        }
        let metric_key = match (computation_mode, metric_key) {
            (GoalComputationMode::RegisteredMetric, Some(key)) => {
                let key = key.trim();
                if key.chars().count() > MAX_KEY_LEN || !self.metrics.contains(key) {
                    return Err(GamificationError::MetricKeyUnknown(key.to_string()));
                }
                Some(key)
            }
            (GoalComputationMode::RegisteredMetric, None) => {
                return Err(GamificationError::MetricKeyUnknown(String::new()));
            }
            (GoalComputationMode::Manual, Some(key)) => {
                let key = key.trim();
                if !key.is_empty() {
                    if !self.metrics.contains(key) {
                        return Err(GamificationError::MetricKeyUnknown(key.to_string()));
                    }
                    Some(key)
                } else {
                    None
                }
            }
            (GoalComputationMode::Manual, None) => None,
        };

        let mut tx = self.pool.begin().await?;
        let def = GamificationRepository::insert_goal_definition(
            &mut tx,
            Uuid::new_v4(),
            name,
            description,
            suffix,
            monetary,
            &computation_mode.to_string(),
            metric_key,
            &condition.to_string(),
            &display_mode.to_string(),
        )
        .await?;
        tx.commit().await?;
        Ok(def.id)
    }

    // ── membership reconciliation (declarative) ──────────────────────────────

    /// Expand the challenge's declarative membership rules into the
    /// materialized ledger: explicit `user_ids`, holders of
    /// `include_badge_ids`, and — when toggled — every user from the
    /// composition-registered directory. Leavers' memberships retire and
    /// their goals CANCEL (never unlink). Returns (added, leavers).
    async fn reconcile_membership(
        &self,
        conn: &mut PgConnection,
        challenge: &crate::domain::entity::GamificationChallenge,
        counters: Option<&mut (usize, usize)>,
    ) -> Result<(usize, usize), GamificationError> {
        let mut target: HashSet<Uuid> = HashSet::new();
        let mut explicit: HashSet<Uuid> = HashSet::new();

        for id in uuid_array(&challenge.user_ids) {
            explicit.insert(id);
            target.insert(id);
        }
        let badge_filter = uuid_array(&challenge.include_badge_ids);
        if !badge_filter.is_empty() {
            for id in GamificationRepository::badge_holders(conn, &badge_filter).await? {
                target.insert(id);
            }
        }
        if challenge.include_all_users {
            let directory = self.directory.as_ref().ok_or(GamificationError::DirectoryNotRegistered)?;
            for id in directory() {
                target.insert(id);
            }
        }

        let before: HashSet<Uuid> =
            GamificationRepository::live_members(conn, challenge.id).await?.into_iter().collect();

        let mut added = 0usize;
        for id in &target {
            let source = if explicit.contains(id) { "explicit" } else { "rule_expansion" };
            if GamificationRepository::insert_membership(conn, Uuid::new_v4(), challenge.id, *id, source)
                .await?
            {
                added += 1;
            }
        }

        let leavers: Vec<Uuid> = before.difference(&target).copied().collect();
        let keep: Vec<Uuid> = target.into_iter().collect();
        GamificationRepository::retire_memberships(conn, challenge.id, &keep).await?;
        if !leavers.is_empty() {
            GamificationRepository::cancel_goals_of_users(conn, challenge.id, &leavers).await?;
        }

        if let Some((c_added, c_canceled)) = counters {
            *c_added += added;
            *c_canceled += leavers.len();
        }
        Ok((added, leavers.len()))
    }

    /// Generate the missing goals for every line x member at `current = 0`
    /// (the explicit first evaluation follows; goals are never seeded
    /// dirty). A member whose line-goal still covers today is skipped; a
    /// passed window does not block recurring regeneration.
    async fn generate_goals(
        &self,
        conn: &mut PgConnection,
        challenge: &crate::domain::entity::GamificationChallenge,
        today: NaiveDate,
        counters: Option<&mut usize>,
    ) -> Result<usize, GamificationError> {
        let members = GamificationRepository::live_members(conn, challenge.id).await?;
        if members.is_empty() {
            return Ok(0);
        }
        let lines = GamificationRepository::challenge_lines(conn, challenge.id).await?;
        let (window_start, window_end) =
            period_window(challenge.period, today, challenge.end_date);

        let mut generated = 0usize;
        for (line_id, definition_id, target) in &lines {
            for member in &members {
                if GamificationRepository::find_active_goal_for_line(conn, *line_id, *member, today)
                    .await?
                    .is_some()
                {
                    continue;
                }
                GamificationRepository::insert_goal(
                    conn,
                    Uuid::new_v4(),
                    *definition_id,
                    *member,
                    *line_id,
                    challenge.id,
                    window_start,
                    window_end,
                    *target,
                )
                .await?;
                generated += 1;
            }
        }
        if let Some(counter) = counters {
            *counter += generated;
        }
        Ok(generated)
    }

    // ── challenge transitions (machine #1 — through verbs only) ─────────────

    /// `draft → inprogress`: materialize membership from the declarative
    /// rules and generate goals at `current = 0`. The report clock arms
    /// when a cadence is set.
    pub async fn start_challenge(&self, challenge_id: Uuid) -> Result<(), GamificationError> {
        let mut tx = self.pool.begin().await?;
        let challenge = GamificationRepository::find_challenge(&mut tx, challenge_id)
            .await?
            .ok_or(GamificationError::ChallengeNotFound)?;
        if challenge.state != ChallengeState::Draft {
            return Err(GamificationError::ChallengeInvalidState);
        }
        let today = Utc::now().date_naive();
        self.reconcile_membership(&mut tx, &challenge, None).await?;
        self.generate_goals(&mut tx, &challenge, today, None).await?;
        GamificationRepository::set_challenge_state(&mut tx, challenge_id, "inprogress").await?;
        if challenge.report_frequency != ReportFrequency::Never {
            let next = advance_report_date(today, challenge.report_frequency);
            GamificationRepository::stamp_report_dates(&mut tx, challenge_id, today, next).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// `inprogress → done`: close passed windows (goals land `failed`),
    /// then force the podium reward check.
    pub async fn close_challenge(&self, challenge_id: Uuid) -> Result<usize, GamificationError> {
        let mut tx = self.pool.begin().await?;
        let challenge = GamificationRepository::find_challenge(&mut tx, challenge_id)
            .await?
            .ok_or(GamificationError::ChallengeNotFound)?;
        if challenge.state != ChallengeState::Inprogress {
            return Err(GamificationError::ChallengeInvalidState);
        }
        let today = Utc::now().date_naive();
        let failed = GamificationRepository::window_close_passed_goals(
            &mut tx,
            challenge_id,
            today,
        )
        .await?;
        let (granted, deduped) = self.podium_rewards(&mut tx, &challenge).await?;
        GamificationRepository::set_challenge_state(&mut tx, challenge_id, "done").await?;
        tx.commit().await?;
        tracing::info!(
            challenge = %challenge_id,
            failed, granted, deduped,
            "challenge closed — podium rewards forced"
        );
        Ok(granted)
    }

    /// `done → draft` — REFUSED while any live goal of the challenge is
    /// inprogress (rule R-G8): no silent goal loss.
    pub async fn reset_challenge(&self, challenge_id: Uuid) -> Result<(), GamificationError> {
        let mut tx = self.pool.begin().await?;
        let challenge = GamificationRepository::find_challenge(&mut tx, challenge_id)
            .await?
            .ok_or(GamificationError::ChallengeNotFound)?;
        if challenge.state != ChallengeState::Done {
            return Err(GamificationError::ChallengeInvalidState);
        }
        if GamificationRepository::has_inprogress_goals(&mut tx, challenge_id).await? {
            return Err(GamificationError::ChallengeHasLiveGoals);
        }
        GamificationRepository::set_challenge_state(&mut tx, challenge_id, "draft").await?;
        tx.commit().await?;
        Ok(())
    }

    // ── goal transitions (machine #2) ────────────────────────────────────────

    /// Evaluate one goal through its registered metric (the ONLY computed
    /// mode): write the fresh value, start drafts, land `reached` when the
    /// condition is met. `window_close` handles the unmet-passed-window
    /// edge in the daily sweep.
    pub async fn evaluate_goal(&self, goal_id: Uuid) -> Result<GoalState, GamificationError> {
        let mut tx = self.pool.begin().await?;
        let goal = GamificationRepository::find_goal(&mut tx, goal_id)
            .await?
            .ok_or(GamificationError::GoalNotFound)?;
        let definition =
            GamificationRepository::find_goal_definition(&mut tx, goal.definition_id)
                .await?
                .ok_or(GamificationError::DefinitionNotFound)?;
        if definition.computation_mode != GoalComputationMode::RegisteredMetric {
            return Err(GamificationError::GoalNotComputed);
        }
        let state = self
            .evaluate_goal_row(&mut tx, &goal, &definition, Utc::now().date_naive())
            .await?;
        tx.commit().await?;
        Ok(state)
    }

    /// The evaluation core (shared with the daily check, on its
    /// connection).
    async fn evaluate_goal_row(
        &self,
        conn: &mut PgConnection,
        goal: &GamificationGoal,
        definition: &GamificationGoalDefinition,
        today: NaiveDate,
    ) -> Result<GoalState, GamificationError> {
        if !matches!(goal.state, GoalState::Draft | GoalState::Inprogress) {
            return Err(GamificationError::GoalInvalidState);
        }
        let key = definition.metric_key.as_deref().ok_or_else(|| {
            GamificationError::MetricKeyUnknown(String::new())
        })?;
        let compute = self.metric(key)?;
        let window = (goal.start_date, goal.end_date);
        let value = compute(goal.user_id, window);

        GamificationRepository::set_goal_current(conn, goal.id, value, today).await?;
        let mut state = goal.state;
        if state == GoalState::Draft {
            // The explicit first evaluation activates the goal
            // (definition/user frozen from here — rule R-G7).
            GamificationRepository::set_goal_state(conn, goal.id, "inprogress").await?;
            state = GoalState::Inprogress;
        }
        if condition_met(definition.condition, value, goal.target) {
            GamificationRepository::set_goal_state(conn, goal.id, "reached").await?;
            state = GoalState::Reached;
        }
        Ok(state)
    }

    /// Human-entered value for a MANUAL goal (reminder-driven): writes the
    /// value, clears the reminder flag, and lands `reached` when the
    /// condition now holds.
    pub async fn set_goal_current(
        &self,
        goal_id: Uuid,
        value: Decimal,
    ) -> Result<GoalState, GamificationError> {
        let mut tx = self.pool.begin().await?;
        let goal = GamificationRepository::find_goal(&mut tx, goal_id)
            .await?
            .ok_or(GamificationError::GoalNotFound)?;
        let definition =
            GamificationRepository::find_goal_definition(&mut tx, goal.definition_id)
                .await?
                .ok_or(GamificationError::DefinitionNotFound)?;
        if definition.computation_mode != GoalComputationMode::Manual {
            return Err(GamificationError::GoalNotManual);
        }
        if !matches!(goal.state, GoalState::Draft | GoalState::Inprogress) {
            return Err(GamificationError::GoalInvalidState);
        }

        let today = Utc::now().date_naive();
        GamificationRepository::set_goal_current(&mut tx, goal_id, value, today).await?;
        let mut state = goal.state;
        if state == GoalState::Draft {
            GamificationRepository::set_goal_state(&mut tx, goal_id, "inprogress").await?;
            state = GoalState::Inprogress;
        }
        if condition_met(definition.condition, value, goal.target) {
            GamificationRepository::set_goal_state(&mut tx, goal_id, "reached").await?;
            state = GoalState::Reached;
        }
        tx.commit().await?;
        Ok(state)
    }

    /// The validated self-service reach: for registered-metric goals the
    /// metric is RE-COMPUTED first and an unmet condition is a loud
    /// refusal — never a silent revert on the next evaluation (D-16).
    pub async fn manual_reach(&self, goal_id: Uuid) -> Result<(), GamificationError> {
        let mut tx = self.pool.begin().await?;
        let goal = GamificationRepository::find_goal(&mut tx, goal_id)
            .await?
            .ok_or(GamificationError::GoalNotFound)?;
        let definition =
            GamificationRepository::find_goal_definition(&mut tx, goal.definition_id)
                .await?
                .ok_or(GamificationError::DefinitionNotFound)?;
        if goal.state != GoalState::Inprogress {
            return Err(GamificationError::GoalInvalidState);
        }

        let today = Utc::now().date_naive();
        let current = match definition.computation_mode {
            GoalComputationMode::RegisteredMetric => {
                // Re-compute and persist, then validate against the fresh
                // value — the "validated" reach.
                let key = definition
                    .metric_key
                    .as_deref()
                    .ok_or_else(|| GamificationError::MetricKeyUnknown(String::new()))?;
                let compute = self.metric(key)?;
                let value = compute(goal.user_id, (goal.start_date, goal.end_date));
                GamificationRepository::set_goal_current(&mut tx, goal_id, value, today).await?;
                value
            }
            GoalComputationMode::Manual => goal.current,
        };

        if !condition_met(definition.condition, current, goal.target) {
            return Err(GamificationError::GoalCriteriaUnmet);
        }
        GamificationRepository::set_goal_state(&mut tx, goal_id, "reached").await?;
        tx.commit().await?;
        Ok(())
    }

    /// Cancel a goal (the leaver sweep's edge for one row): rows stay,
    /// never unlinked.
    pub async fn cancel_goal(&self, goal_id: Uuid) -> Result<(), GamificationError> {
        let mut tx = self.pool.begin().await?;
        let goal = GamificationRepository::find_goal(&mut tx, goal_id)
            .await?
            .ok_or(GamificationError::GoalNotFound)?;
        if !matches!(goal.state, GoalState::Inprogress | GoalState::Reached) {
            return Err(GamificationError::GoalInvalidState);
        }
        GamificationRepository::set_goal_state(&mut tx, goal_id, "canceled").await?;
        tx.commit().await?;
        Ok(())
    }

    // ── rewards ──────────────────────────────────────────────────────────────

    /// Realtime reward check: every member whose goals for this challenge
    /// are ALL reached (and there is at least one) earns the reward badge
    /// through `grant_system` — realtime and podium dedup share the ONE
    /// `grant_key` mechanism.
    async fn realtime_rewards(
        &self,
        conn: &mut PgConnection,
        challenge: &crate::domain::entity::GamificationChallenge,
    ) -> Result<(usize, usize), GamificationError> {
        let Some(badge_id) = challenge.reward_badge_id else {
            return Ok((0, 0));
        };
        if !challenge.reward_realtime {
            return Ok((0, 0));
        }
        let goals = GamificationRepository::challenge_goals(conn, challenge.id).await?;
        let mut per_user: HashMap<Uuid, Vec<&GamificationGoal>> = HashMap::new();
        for goal in &goals {
            per_user.entry(goal.user_id).or_default().push(goal);
        }
        let mut granted = 0usize;
        let mut deduped = 0usize;
        for (user, goals) in per_user {
            if goals.is_empty() || !goals.iter().all(|g| g.state == GoalState::Reached) {
                continue;
            }
            let key = format!("challenge:{}:{badge_id}:{user}", challenge.id);
            let view = self
                .grant_system_on(conn, badge_id, user, BadgeGrantKind::Challenge, &key, Some(challenge.id), None)
                .await?;
            if view.created {
                granted += 1;
            } else {
                deduped += 1;
            }
        }
        Ok((granted, deduped))
    }

    /// The podium (end-of-challenge): rank members by (all-reached, total
    /// completeness) descending; grant the first/second/third badges —
    /// walking only fully-succeeding users unless `reward_failure`.
    async fn podium_rewards(
        &self,
        conn: &mut PgConnection,
        challenge: &crate::domain::entity::GamificationChallenge,
    ) -> Result<(usize, usize), GamificationError> {
        let goals = GamificationRepository::challenge_goals(conn, challenge.id).await?;
        // Per-user (all_reached, avg completeness) — needs definitions for
        // the completeness condition.
        let mut defs: HashMap<Uuid, GoalCondition> = HashMap::new();
        let mut per_user: HashMap<Uuid, Vec<&GamificationGoal>> = HashMap::new();
        for goal in &goals {
            per_user.entry(goal.user_id).or_default().push(goal);
            if !defs.contains_key(&goal.definition_id) {
                if let Some(def) =
                    GamificationRepository::find_goal_definition(conn, goal.definition_id).await?
                {
                    defs.insert(goal.definition_id, def.condition);
                }
            }
        }
        let mut ranked: Vec<(bool, Decimal, Uuid)> = per_user
            .into_iter()
            .map(|(user, goals)| {
                let all_reached =
                    !goals.is_empty() && goals.iter().all(|g| g.state == GoalState::Reached);
                let total: Decimal = goals
                    .iter()
                    .map(|g| {
                        let condition = *defs.get(&g.definition_id).unwrap_or(&GoalCondition::Higher);
                        completeness(condition, g.current, g.target)
                    })
                    .sum();
                let avg = if goals.is_empty() {
                    Decimal::ZERO
                } else {
                    total / Decimal::from(goals.len())
                };
                (all_reached, avg, user)
            })
            .collect();
        ranked.sort_by(|a, b| {
            b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(a.2.cmp(&b.2))
        });

        let podium: [Option<Uuid>; 3] = [
            challenge.reward_first_badge_id,
            challenge.reward_second_badge_id,
            challenge.reward_third_badge_id,
        ];
        let mut granted = 0usize;
        let mut deduped = 0usize;
        for (place, badge_opt) in podium.iter().enumerate() {
            let Some(badge_id) = *badge_opt else { continue };
            // takewhile fully-succeeding unless reward_failure admits the
            // best-effort podium.
            let Some(&(all_reached, _, user)) = ranked.get(place) else { break };
            if !all_reached && !challenge.reward_failure {
                break;
            }
            let key = format!("challenge:{}:{badge_id}:{user}", challenge.id);
            let view = self
                .grant_system_on(conn, badge_id, user, BadgeGrantKind::Challenge, &key, Some(challenge.id), None)
                .await?;
            if view.created {
                granted += 1;
            } else {
                deduped += 1;
            }
        }
        Ok((granted, deduped))
    }

    /// The report payload for a due challenge (the host-relayed
    /// `ChallengeReportDue` fact material — delivery is host composition).
    async fn compute_report(
        conn: &mut PgConnection,
        challenge: &crate::domain::entity::GamificationChallenge,
    ) -> serde_json::Value {
        let goals = GamificationRepository::challenge_goals(conn, challenge.id)
            .await
            .unwrap_or_default();
        let members = GamificationRepository::live_members(conn, challenge.id)
            .await
            .unwrap_or_default();
        serde_json::json!({
            "challenge": challenge.name,
            "members": members.len(),
            "goals": goals.len(),
            "reached": goals.iter().filter(|g| g.state == GoalState::Reached).count(),
            "inprogress": goals.iter().filter(|g| g.state == GoalState::Inprogress).count(),
            "failed": goals.iter().filter(|g| g.state == GoalState::Failed).count(),
        })
    }

    // ── the daily check (the ONE scheduled job) ──────────────────────────────

    /// The daily challenge check (cron `0 5 * * *`, handler
    /// `gamification::daily_challenge_check`): claims due challenges
    /// `FOR UPDATE SKIP LOCKED` one at a time (two workers never
    /// double-run a challenge) and processes each in its own transaction
    /// (commit per challenge — every effect idempotent, so the replay
    /// window is bounded).
    ///
    /// Per challenge: start drafts whose `start_date` arrived (through the
    /// start transition — membership materializes, goals generate at
    /// current = 0); close inprogress challenges whose `end_date` passed
    /// (windows close `failed`, podium rewards force); for the still
    /// running: reconcile membership (leavers' goals cancel), generate
    /// missing goals, evaluate registered-metric goals (NO presence gate),
    /// check realtime rewards; advance the report clock and compute the
    /// due-report payload.
    pub async fn daily_challenge_check(&self, today: NaiveDate) -> Result<DailyCheckReport, GamificationError> {
        let mut report = DailyCheckReport::default();
        // Challenges THIS run already processed. An inprogress challenge is
        // always nominally due, so the exclusion set is what bounds the run
        // to one pass per challenge (concurrent workers still coordinate
        // through the SKIP LOCKED claim).
        let mut processed: Vec<Uuid> = Vec::new();

        loop {
            let mut tx = self.pool.begin().await?;
            let claimed =
                GamificationRepository::claim_next_due_challenge(&mut tx, today, &processed).await?;
            let Some(challenge_id) = claimed else {
                tx.commit().await?;
                break;
            };
            processed.push(challenge_id);

            let challenge = GamificationRepository::find_challenge(&mut tx, challenge_id)
                .await?
                .ok_or(GamificationError::ChallengeNotFound)?;
            report.challenges_processed += 1;

            // The start edge, by date, THROUGH the transition — then fall
            // through to the inprogress sweep in the SAME iteration, so a
            // challenge that becomes due today starts and is evaluated in
            // one run.
            let mut state = challenge.state;
            if state == ChallengeState::Draft
                && challenge.start_date.is_some_and(|d| d <= today)
            {
                let mut counters = (0usize, 0usize);
                self.reconcile_membership(&mut tx, &challenge, Some(&mut counters)).await?;
                let mut generated = 0usize;
                self.generate_goals(&mut tx, &challenge, today, Some(&mut generated)).await?;
                GamificationRepository::set_challenge_state(&mut tx, challenge_id, "inprogress").await?;
                report.challenges_started += 1;
                report.goals_generated += generated;
                report.goals_canceled += counters.1;
                state = ChallengeState::Inprogress;
            }

            if state == ChallengeState::Inprogress {
                let closing = challenge.end_date.is_some_and(|d| d < today);
                if closing {
                    let failed = GamificationRepository::window_close_passed_goals(
                        &mut tx,
                        challenge_id,
                        today,
                    )
                    .await?;
                    // Podium first, then the every-succeeding-user badge
                    // (both share the grant_key mechanism; both no-op when
                    // unconfigured).
                    let (mut granted, mut deduped) =
                        self.podium_rewards(&mut tx, &challenge).await?;
                    let (g, d) = self.realtime_rewards(&mut tx, &challenge).await?;
                    granted += g;
                    deduped += d;
                    GamificationRepository::set_challenge_state(&mut tx, challenge_id, "done").await?;
                    report.challenges_closed += 1;
                    report.goals_failed += failed as usize;
                    report.rewards_granted += granted;
                    report.rewards_deduped += deduped;
                } else {
                    let mut counters = (0usize, 0usize);
                    self.reconcile_membership(&mut tx, &challenge, Some(&mut counters)).await?;
                    let mut generated = 0usize;
                    self.generate_goals(&mut tx, &challenge, today, Some(&mut generated)).await?;

                    // Evaluate registered-metric goals (no presence gate —
                    // absent users' goals still compute).
                    let goals = GamificationRepository::challenge_goals(&mut tx, challenge_id).await?;
                    let mut evaluated = 0usize;
                    for goal in goals {
                        if !matches!(goal.state, GoalState::Draft | GoalState::Inprogress) {
                            continue;
                        }
                        let Some(definition) = GamificationRepository::find_goal_definition(
                            &mut tx,
                            goal.definition_id,
                        )
                        .await?
                        else {
                            continue;
                        };
                        if definition.computation_mode != GoalComputationMode::RegisteredMetric {
                            continue;
                        }
                        if self
                            .evaluate_goal_row(&mut tx, &goal, &definition, today)
                            .await
                            .is_ok()
                        {
                            evaluated += 1;
                        }
                    }
                    let (granted, deduped) = self.realtime_rewards(&mut tx, &challenge).await?;
                    report.goals_generated += generated;
                    report.goals_canceled += counters.1;
                    report.goals_evaluated += evaluated;
                    report.rewards_granted += granted;
                    report.rewards_deduped += deduped;
                }
            }

            // The report cadence sweep (any live-or-closed state with a due
            // clock — closed challenges keep reporting until they run out).
            if challenge.report_frequency != ReportFrequency::Never
                && challenge.next_report_date.map_or(true, |d| d <= today)
            {
                let summary = Self::compute_report(&mut tx, &challenge).await;
                let next = advance_report_date(today, challenge.report_frequency);
                GamificationRepository::stamp_report_dates(&mut tx, challenge_id, today, next).await?;
                report.reports_due.push(ChallengeReportDue {
                    challenge_id,
                    mode: challenge.visibility_mode.to_string(),
                    summary,
                });
            }

            tx.commit().await?;
        }

        Ok(report)
    }
}
