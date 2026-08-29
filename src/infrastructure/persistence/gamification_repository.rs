//! `GamificationRepository` — the hand-written SQL behind the gamification
//! write paths (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Module rule: services orchestrate, repositories hold SQL. Runtime queries
//! — no `.sqlx` macros, so no compile-time cache is needed. Every method
//! takes a connection inside the caller's transaction; every read filters on
//! `metadata->>'deleted_at' IS NULL` (the soft-delete convention) EXCEPT the
//! karma ledger, whose rows are immutable by trigger (nothing is ever
//! soft-deleted there, so the filter would be dead weight).
//!
//! What lives here and why:
//!
//! - **The karma ledger has NO update/delete SQL.** Not hidden, not
//!   feature-gated — absent. The DB trigger
//!   (`gamification_karma_trackings_immutable`) is the backstop; the
//!   repository is the front promise. Balance is a PROJECTION: the latest
//!   row per user by `(tracking_date DESC, id DESC)`, and appends serialize
//!   per user on a transaction-scoped advisory lock so `old_value` never
//!   goes stale under concurrency.
//! - **Exact-once keyed grants** ride `INSERT ... ON CONFLICT DO NOTHING`
//!   over the partial UNIQUE index on `grant_key` (live rows): a concurrent
//!   duplicate delivery affects zero rows and the service converges on the
//!   winner — the certification contract's dedup leg.
//! - **The daily check claims due challenges** `FOR UPDATE SKIP LOCKED`,
//!   one at a time — two workers never double-run a challenge's rewards.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::{
    GamificationBadge, GamificationBadgeUser, GamificationChallenge, GamificationGoal,
    GamificationGoalDefinition, GamificationKarmaRank,
};

/// Hand-written gamification SQL. Services orchestrate; this holds SQL.
pub struct GamificationRepository;

impl GamificationRepository {
    pub fn new() -> Self {
        Self
    }

    // ── the karma ledger (append-only) ────────────────────────────────────────

    /// Serialize appends for one user inside this transaction (the balance
    /// projection read and the INSERT must be atomic per user, or two
    /// concurrent appends would read the same `old_value`).
    pub async fn lock_user_ledger(conn: &mut PgConnection, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
            .bind(user_id.to_string())
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// THE balance projection: the user's latest ledger row's `new_value`
    /// (never a sum). None when no row exists (balance zero).
    pub async fn balance(conn: &mut PgConnection, user_id: Uuid) -> Result<Option<i32>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT new_value FROM engagement.gamification_karma_trackings
               WHERE user_id = $1
               ORDER BY tracking_date DESC, id DESC
               LIMIT 1"#,
        )
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Append one ledger row (the ONLY write this table ever receives).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_ledger_row(
        conn: &mut PgConnection,
        id: Uuid,
        user_id: Uuid,
        old_value: i32,
        new_value: i32,
        reason: &str,
        origin_kind: &str,
        origin_id: Option<Uuid>,
        origin_label: Option<&str>,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(
            r#"INSERT INTO engagement.gamification_karma_trackings
                   (id, user_id, old_value, new_value, reason, origin_kind,
                    origin_id, origin_label, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                       jsonb_build_object('created_at', to_jsonb(now())))
               RETURNING id"#,
        )
        .bind(id)
        .bind(user_id)
        .bind(old_value)
        .bind(new_value)
        .bind(reason)
        .bind(origin_kind)
        .bind(origin_id)
        .bind(origin_label)
        .fetch_one(&mut *conn)
        .await
    }

    /// A user's ledger row count (test/inspection arm).
    pub async fn ledger_row_count(
        conn: &mut PgConnection,
        user_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT count(*) FROM engagement.gamification_karma_trackings
               WHERE user_id = $1"#,
        )
        .bind(user_id)
        .fetch_one(&mut *conn)
        .await
    }

    /// The leaderboard projection: every user's LATEST row, ranked by
    /// `new_value` DESC (the SAME projection as the balance — the upstream
    /// SUM-window leaderboard is the dropped half of the two-semantics
    /// conflict).
    pub async fn leaderboard(
        conn: &mut PgConnection,
        limit: i64,
    ) -> Result<Vec<(Uuid, i32)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, i32)>(
            r#"SELECT user_id, new_value FROM (
                   SELECT DISTINCT ON (user_id) user_id, new_value
                   FROM engagement.gamification_karma_trackings
                   ORDER BY user_id, tracking_date DESC, id DESC
               ) latest
               ORDER BY new_value DESC, user_id
               LIMIT $1"#,
        )
        .bind(limit)
        .fetch_all(&mut *conn)
        .await
    }

    // ── the rank ladder ───────────────────────────────────────────────────────

    /// The rank a balance falls into (karma_min DESC walk — the ONE
    /// read-side derivation).
    pub async fn rank_for_balance(
        conn: &mut PgConnection,
        balance: i32,
    ) -> Result<Option<GamificationKarmaRank>, sqlx::Error> {
        sqlx::query_as::<_, GamificationKarmaRank>(
            r#"SELECT * FROM engagement.gamification_karma_ranks
               WHERE karma_min <= $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY karma_min DESC
               LIMIT 1"#,
        )
        .bind(balance)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The next rank above a balance (display arm; None at the top).
    pub async fn next_rank(
        conn: &mut PgConnection,
        balance: i32,
    ) -> Result<Option<GamificationKarmaRank>, sqlx::Error> {
        sqlx::query_as::<_, GamificationKarmaRank>(
            r#"SELECT * FROM engagement.gamification_karma_ranks
               WHERE karma_min > $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY karma_min ASC
               LIMIT 1"#,
        )
        .bind(balance)
        .fetch_optional(&mut *conn)
        .await
    }

    // ── badges: definitions ──────────────────────────────────────────────────

    pub async fn find_badge(
        conn: &mut PgConnection,
        badge_id: Uuid,
    ) -> Result<Option<GamificationBadge>, sqlx::Error> {
        sqlx::query_as::<_, GamificationBadge>(
            r#"SELECT * FROM engagement.gamification_badges
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(badge_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Resolve a badge by its stable key (the name — the certification
    /// contract's `badge_key`).
    pub async fn find_badge_by_name(
        conn: &mut PgConnection,
        name: &str,
    ) -> Result<Option<GamificationBadge>, sqlx::Error> {
        sqlx::query_as::<_, GamificationBadge>(
            r#"SELECT * FROM engagement.gamification_badges
               WHERE name = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Distinct badges from a set that a user HOLDS a live grant of (the
    /// `having` ladder arm — the caller compares against the full set).
    pub async fn count_badges_held(
        conn: &mut PgConnection,
        user_id: Uuid,
        badge_ids: &[Uuid],
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT count(DISTINCT badge_id) FROM engagement.gamification_badge_users
               WHERE recipient_user_id = $1
                 AND badge_id = ANY($2)
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(user_id)
        .bind(badge_ids)
        .fetch_one(&mut *conn)
        .await
    }

    /// The sender's peer grants of ONE badge this calendar month (the cap
    /// arm — the single grouped query that replaces the upstream per-badge
    /// owner loop).
    pub async fn peer_grants_this_month(
        conn: &mut PgConnection,
        badge_id: Uuid,
        sender_user_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT count(*) FROM engagement.gamification_badge_users
               WHERE badge_id = $1
                 AND sender_user_id = $2
                 AND grant_kind = 'peer'
                 AND granted_at >= date_trunc('month', now())
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(badge_id)
        .bind(sender_user_id)
        .fetch_one(&mut *conn)
        .await
    }

    /// A badge's grant stats in ONE grouped query (totals + this month).
    pub async fn badge_grant_stats(
        conn: &mut PgConnection,
        badge_id: Uuid,
    ) -> Result<(i64, i64, i64), sqlx::Error> {
        sqlx::query_as::<_, (i64, i64, i64)>(
            r#"SELECT
                   count(*),
                   count(DISTINCT recipient_user_id),
                   count(*) FILTER (WHERE granted_at >= date_trunc('month', now()))
               FROM engagement.gamification_badge_users
               WHERE badge_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(badge_id)
        .fetch_one(&mut *conn)
        .await
    }

    // ── badges: the two grant verbs ──────────────────────────────────────────

    /// Insert a grant row. `ON CONFLICT DO NOTHING` rides the partial
    /// UNIQUE index on `grant_key` (live rows): a keyed grant delivered
    /// concurrently or replayed affects zero rows — exactly-once. Peer
    /// grants carry a NULL key (no index coverage) and stay
    /// repeatable-under-cap.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_grant(
        conn: &mut PgConnection,
        id: Uuid,
        badge_id: Uuid,
        recipient_user_id: Uuid,
        sender_user_id: Option<Uuid>,
        grant_kind: &str,
        challenge_id: Option<Uuid>,
        comment: Option<&str>,
        level: Option<&str>,
        grant_key: Option<&str>,
    ) -> Result<Option<GamificationBadgeUser>, sqlx::Error> {
        let row = sqlx::query_as::<_, GamificationBadgeUser>(
            r#"INSERT INTO engagement.gamification_badge_users
                   (id, badge_id, recipient_user_id, sender_user_id, grant_kind,
                    challenge_id, comment, level, grant_key, metadata)
               VALUES ($1, $2, $3, $4, $5::badge_grant_kind, $6, $7,
                       $8::badge_level, $9,
                       jsonb_build_object('created_at', to_jsonb(now())))
               ON CONFLICT DO NOTHING
               RETURNING *"#,
        )
        .bind(id)
        .bind(badge_id)
        .bind(recipient_user_id)
        .bind(sender_user_id)
        .bind(grant_kind)
        .bind(challenge_id)
        .bind(comment)
        .bind(level)
        .bind(grant_key)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(row)
    }

    /// The live grant row a `grant_key` already holds (the dedup arm's
    /// convergence read).
    pub async fn find_grant_by_key(
        conn: &mut PgConnection,
        grant_key: &str,
    ) -> Result<Option<GamificationBadgeUser>, sqlx::Error> {
        sqlx::query_as::<_, GamificationBadgeUser>(
            r#"SELECT * FROM engagement.gamification_badge_users
               WHERE grant_key = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(grant_key)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Does the recipient hold a LIVE grant of this badge (level counts and
    /// ladder checks)?
    pub async fn holds_badge(
        conn: &mut PgConnection,
        recipient_user_id: Uuid,
        badge_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let hit: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1::int8 FROM engagement.gamification_badge_users
               WHERE recipient_user_id = $1 AND badge_id = $2
                 AND (metadata->>'deleted_at') IS NULL
               LIMIT 1"#,
        )
        .bind(recipient_user_id)
        .bind(badge_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(hit.is_some())
    }

    // ── goal definitions ─────────────────────────────────────────────────────

    /// Insert a goal definition through the typed verb (which validates
    /// `metric_key` against the registry — the generic route bypass is a
    /// composition concern, not this layer's).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_goal_definition(
        conn: &mut PgConnection,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        suffix: Option<&str>,
        monetary: bool,
        computation_mode: &str,
        metric_key: Option<&str>,
        condition: &str,
        display_mode: &str,
    ) -> Result<GamificationGoalDefinition, sqlx::Error> {
        sqlx::query_as::<_, GamificationGoalDefinition>(
            r#"INSERT INTO engagement.gamification_goal_definitions
                   (id, name, description, suffix, monetary, computation_mode,
                    metric_key, condition, display_mode, metadata)
               VALUES ($1, $2, $3, $4, $5, $6::goal_computation_mode, $7,
                       $8::goal_condition, $9::goal_display_mode,
                       jsonb_build_object('created_at', to_jsonb(now())))
               RETURNING *"#,
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(suffix)
        .bind(monetary)
        .bind(computation_mode)
        .bind(metric_key)
        .bind(condition)
        .bind(display_mode)
        .fetch_one(&mut *conn)
        .await
    }

    pub async fn find_goal_definition(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<GamificationGoalDefinition>, sqlx::Error> {
        sqlx::query_as::<_, GamificationGoalDefinition>(
            r#"SELECT * FROM engagement.gamification_goal_definitions
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    // ── challenges ───────────────────────────────────────────────────────────

    pub async fn find_challenge(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<GamificationChallenge>, sqlx::Error> {
        sqlx::query_as::<_, GamificationChallenge>(
            r#"SELECT * FROM engagement.gamification_challenges
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Claim ONE due challenge `FOR UPDATE SKIP LOCKED` — the pickup lock:
    /// due means draft-and-startable, inprogress (the check's sweep), or
    /// carrying a due report. `exclude` are the challenges THIS run already
    /// processed (an inprogress challenge is always nominally due; the
    /// exclusion is what bounds the run to one pass). None when every due
    /// row is taken or excluded.
    pub async fn claim_next_due_challenge(
        conn: &mut PgConnection,
        today: NaiveDate,
        exclude: &[Uuid],
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM engagement.gamification_challenges
               WHERE (metadata->>'deleted_at') IS NULL
                 AND NOT (id = ANY($2))
                 AND (
                      (state = 'draft' AND start_date IS NOT NULL AND start_date <= $1)
                   OR state = 'inprogress'
                   OR (state = 'done' AND next_report_date IS NOT NULL AND next_report_date <= $1)
                 )
               ORDER BY id
               FOR UPDATE SKIP LOCKED
               LIMIT 1"#,
        )
        .bind(today)
        .bind(exclude)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Move a challenge's state (the transition verbs' write arm — the only
    /// writer of `state`).
    pub async fn set_challenge_state(
        conn: &mut PgConnection,
        id: Uuid,
        state: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.gamification_challenges
               SET state = $2::challenge_state
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(state)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Stamp the report clock (last + next due date — plain stored state the
    /// daily check owns).
    pub async fn stamp_report_dates(
        conn: &mut PgConnection,
        id: Uuid,
        last_report_date: NaiveDate,
        next_report_date: Option<NaiveDate>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE engagement.gamification_challenges
               SET last_report_date = $2, next_report_date = $3
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(last_report_date)
        .bind(next_report_date)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// A challenge's lines (the goal-generation input).
    pub async fn challenge_lines(
        conn: &mut PgConnection,
        challenge_id: Uuid,
    ) -> Result<Vec<(Uuid, Uuid, Decimal)>, sqlx::Error> {
        sqlx::query_as::<_, (Uuid, Uuid, Decimal)>(
            r#"SELECT id, definition_id, target_goal
               FROM engagement.gamification_challenge_lines
               WHERE challenge_id = $1 AND (metadata->>'deleted_at') IS NULL
               ORDER BY sequence, id"#,
        )
        .bind(challenge_id)
        .fetch_all(&mut *conn)
        .await
    }

    // ── membership (the materialized ledger) ─────────────────────────────────

    /// Materialize one membership row (idempotent on the live
    /// `(challenge, user)` partial-unique — replays converge).
    pub async fn insert_membership(
        conn: &mut PgConnection,
        id: Uuid,
        challenge_id: Uuid,
        user_id: Uuid,
        source: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"INSERT INTO engagement.gamification_challenge_memberships
                   (id, challenge_id, user_id, source, metadata)
               VALUES ($1, $2, $3, $4::membership_source,
                       jsonb_build_object('created_at', to_jsonb(now())))
               ON CONFLICT DO NOTHING"#,
        )
        .bind(id)
        .bind(challenge_id)
        .bind(user_id)
        .bind(source)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// The live member set of a challenge (the reconciliation's baseline).
    pub async fn live_members(
        conn: &mut PgConnection,
        challenge_id: Uuid,
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT user_id FROM engagement.gamification_challenge_memberships
               WHERE challenge_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(challenge_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// Soft-delete the live membership rows of users who LEFT the
    /// declarative set (the reconciliation's convergence arm — the goals are
    /// canceled by the service, never unlinked).
    pub async fn retire_memberships(
        conn: &mut PgConnection,
        challenge_id: Uuid,
        keep: &[Uuid],
    ) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.gamification_challenge_memberships
               SET metadata = metadata || jsonb_build_object(
                       'deleted_at', to_jsonb(now()))
               WHERE challenge_id = $1
                 AND (metadata->>'deleted_at') IS NULL
                 AND NOT (user_id = ANY($2))"#,
        )
        .bind(challenge_id)
        .bind(keep)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() as i64)
    }

    /// Distinct holders of any badge in a set (the `include_badge_ids`
    /// expansion arm).
    pub async fn badge_holders(
        conn: &mut PgConnection,
        badge_ids: &[Uuid],
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT recipient_user_id FROM engagement.gamification_badge_users
               WHERE badge_id = ANY($1) AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(badge_ids)
        .fetch_all(&mut *conn)
        .await
    }

    // ── goals ────────────────────────────────────────────────────────────────

    /// Insert a generated goal (`current = 0`, `state = draft` — the
    /// explicit first evaluation follows; goals are never seeded dirty).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_goal(
        conn: &mut PgConnection,
        id: Uuid,
        definition_id: Uuid,
        user_id: Uuid,
        line_id: Uuid,
        challenge_id: Uuid,
        start_date: NaiveDate,
        end_date: Option<NaiveDate>,
        target: Decimal,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(
            r#"INSERT INTO engagement.gamification_goals
                   (id, definition_id, user_id, line_id, challenge_id,
                    start_date, end_date, target, current, state, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 0, 'draft',
                       jsonb_build_object('created_at', to_jsonb(now())))
               RETURNING id"#,
        )
        .bind(id)
        .bind(definition_id)
        .bind(user_id)
        .bind(line_id)
        .bind(challenge_id)
        .bind(start_date)
        .bind(end_date)
        .bind(target)
        .fetch_one(&mut *conn)
        .await
    }

    /// A line's live, non-canceled goal for one user whose window still
    /// covers `today` (or is open) — the generation dedup. A goal whose
    /// window passed does NOT block a fresh generation (recurring periods).
    pub async fn find_active_goal_for_line(
        conn: &mut PgConnection,
        line_id: Uuid,
        user_id: Uuid,
        today: NaiveDate,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM engagement.gamification_goals
               WHERE line_id = $1 AND user_id = $2
                 AND state <> 'canceled'
                 AND (end_date IS NULL OR end_date >= $3)
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(line_id)
        .bind(user_id)
        .bind(today)
        .fetch_optional(&mut *conn)
        .await
    }

    pub async fn find_goal(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<GamificationGoal>, sqlx::Error> {
        sqlx::query_as::<_, GamificationGoal>(
            r#"SELECT * FROM engagement.gamification_goals
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// The live goals of one challenge (the sweep / reward arms read the
    /// denormalized challenge_id without joining).
    pub async fn challenge_goals(
        conn: &mut PgConnection,
        challenge_id: Uuid,
    ) -> Result<Vec<GamificationGoal>, sqlx::Error> {
        sqlx::query_as::<_, GamificationGoal>(
            r#"SELECT * FROM engagement.gamification_goals
               WHERE challenge_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(challenge_id)
        .fetch_all(&mut *conn)
        .await
    }

    /// Does the challenge have any live goal still inprogress? (The
    /// done→draft reset refusal, rule R-G8.)
    pub async fn has_inprogress_goals(
        conn: &mut PgConnection,
        challenge_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let hit: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1::int8 FROM engagement.gamification_goals
               WHERE challenge_id = $1 AND state = 'inprogress'
                 AND (metadata->>'deleted_at') IS NULL
               LIMIT 1"#,
        )
        .bind(challenge_id)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(hit.is_some())
    }

    /// Write a goal's current value + reminder clock (the evaluation arm).
    pub async fn set_goal_current(
        conn: &mut PgConnection,
        id: Uuid,
        current: Decimal,
        last_update: NaiveDate,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE engagement.gamification_goals
               SET current = $2, last_update = $3, to_update = false
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(current)
        .bind(last_update)
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Move a goal's state (the transition verbs' write arm).
    pub async fn set_goal_state(
        conn: &mut PgConnection,
        id: Uuid,
        state: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.gamification_goals
               SET state = $2::goal_state
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(state)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Cancel the live, non-terminal goals of users who left a challenge
    /// (the leaver sweep — rows stay, never unlinked).
    pub async fn cancel_goals_of_users(
        conn: &mut PgConnection,
        challenge_id: Uuid,
        users: &[Uuid],
    ) -> Result<i64, sqlx::Error> {
        if users.is_empty() {
            return Ok(0);
        }
        let res = sqlx::query(
            r#"UPDATE engagement.gamification_goals
               SET state = 'canceled'::goal_state
               WHERE challenge_id = $1
                 AND user_id = ANY($2)
                 AND state IN ('draft', 'inprogress', 'reached')
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(challenge_id)
        .bind(users)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() as i64)
    }

    /// Close the windows: inprogress goals of a challenge whose `end_date`
    /// passed land `failed` (terminal — no closed flag).
    pub async fn window_close_passed_goals(
        conn: &mut PgConnection,
        challenge_id: Uuid,
        today: NaiveDate,
    ) -> Result<i64, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.gamification_goals
               SET state = 'failed'::goal_state
               WHERE challenge_id = $1
                 AND state = 'inprogress'
                 AND end_date IS NOT NULL AND end_date < $2
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(challenge_id)
        .bind(today)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() as i64)
    }
}

impl Default for GamificationRepository {
    fn default() -> Self {
        Self::new()
    }
}
