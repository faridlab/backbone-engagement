//! `RatingRepository` — the hand-written SQL behind the rating write path
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Module rule: services orchestrate, repositories hold SQL. Runtime queries
//! — no `.sqlx` macros, so no compile-time cache is needed. Every method
//! takes a connection inside the caller's transaction; every read filters on
//! `metadata->>'deleted_at' IS NULL` (the soft-delete convention). The module
//! is unfenced (ADR-0014 posture 4): tenant isolation is the database
//! boundary.
//!
//! What lives here and why:
//!
//! - **Idempotent issuance** rides the partial UNIQUE index on
//!   `(rated_model, rated_record_id, rater_user_id) WHERE consumed = false
//!   AND rater_user_id IS NOT NULL` — find the live unconsumed row first,
//!   insert on miss, and let a concurrent winner surface as the index's
//!   unique-violation error code (the service re-reads to converge).
//! - **Single-use submit** is ONE conditional UPDATE keyed on
//!   `consumed = false AND token_nonce = $ AND token_expires_at > now()`
//!   (rule R-R2): zero rows affected is the refusal, and the caller cannot
//!   distinguish already-consumed / unknown token / expired from the result.

use chrono::{DateTime, Utc};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::EngagementRating;

/// Hand-written rating SQL. Services orchestrate; this holds SQL.
pub struct RatingRepository;

impl RatingRepository {
    pub fn new() -> Self {
        Self
    }

    // ── issuance ──────────────────────────────────────────────────────────────

    /// The live unconsumed row for an identified rater against one target —
    /// the idempotent-issuance probe (rule R-R3; NULL-rater rows are exempt
    /// by design: every anonymous issuance mints its own row).
    pub async fn find_live_unconsumed(
        conn: &mut PgConnection,
        rated_model: &str,
        rated_record_id: Uuid,
        rater_user_id: Uuid,
    ) -> Result<Option<EngagementRating>, sqlx::Error> {
        sqlx::query_as::<_, EngagementRating>(
            r#"SELECT * FROM engagement.engagement_ratings
               WHERE rated_model = $1
                 AND rated_record_id = $2
                 AND rater_user_id = $3
                 AND consumed = false
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(rated_model)
        .bind(rated_record_id)
        .bind(rater_user_id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Insert a rating-request row (unconsumed, token stamped by the
    /// service). The token nonce is the selector leg of the capability; the
    /// MAC lives only in the link the service hands out.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_rating(
        conn: &mut PgConnection,
        id: Uuid,
        rated_model: &str,
        rated_record_id: Uuid,
        parent_rated_model: Option<&str>,
        parent_rated_record_id: Option<Uuid>,
        rated_user_id: Option<Uuid>,
        rater_user_id: Option<Uuid>,
        rater_email: Option<&str>,
        token_nonce: &str,
        token_expires_at: DateTime<Utc>,
        created_by: Option<Uuid>,
    ) -> Result<EngagementRating, sqlx::Error> {
        sqlx::query_as::<_, EngagementRating>(
            r#"INSERT INTO engagement.engagement_ratings
                   (id, rated_model, rated_record_id, parent_rated_model,
                    parent_rated_record_id, rated_user_id, rater_user_id,
                    rater_email, token_nonce, token_expires_at, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, jsonb_build_object(
                   'created_by', $11::text,
                   'created_at', to_jsonb(now())
               ))
               RETURNING *"#,
        )
        .bind(id)
        .bind(rated_model)
        .bind(rated_record_id)
        .bind(parent_rated_model)
        .bind(parent_rated_record_id)
        .bind(rated_user_id)
        .bind(rater_user_id)
        .bind(rater_email)
        .bind(token_nonce)
        .bind(token_expires_at)
        .bind(created_by.map(|u| u.to_string()))
        .fetch_one(&mut *conn)
        .await
    }

    // ── token resolution ─────────────────────────────────────────────────────

    /// Resolve a live rating row by id (the capability's selector is the
    /// (id, nonce) pair; this is the row-load arm of the verify).
    pub async fn find_by_id(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<EngagementRating>, sqlx::Error> {
        sqlx::query_as::<_, EngagementRating>(
            r#"SELECT * FROM engagement.engagement_ratings
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    // ── submit / rotate / reset (the token lifecycle) ────────────────────────

    /// THE single-use submit: ONE conditional UPDATE that flips the row to
    /// consumed and stamps the answer, refusing everything else by simply
    /// affecting zero rows (rule R-R2 — already-consumed, unknown nonce, and
    /// expired are indistinguishable at this boundary).
    pub async fn conditional_submit(
        conn: &mut PgConnection,
        id: Uuid,
        token_nonce: &str,
        rating_value: i32,
        feedback: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.engagement_ratings
               SET rating_value = $3,
                   feedback = $4,
                   consumed = true,
                   rated_on = now()
               WHERE id = $1
                 AND token_nonce = $2
                 AND consumed = false
                 AND token_expires_at > now()
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(token_nonce)
        .bind(rating_value)
        .bind(feedback)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Mint a fresh selector + expiry on an existing row (the rotate verb —
    /// extends a stale link; the old link dies with the old nonce).
    pub async fn rotate_token(
        conn: &mut PgConnection,
        id: Uuid,
        token_nonce: &str,
        token_expires_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.engagement_ratings
               SET token_nonce = $2, token_expires_at = $3
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(token_nonce)
        .bind(token_expires_at)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Reset for a reuse wave: clear the answer and re-arm the capability
    /// with a NEW nonce + expiry (the correction path — value/feedback go to
    /// NULL, consumed back to false).
    pub async fn reset_rating(
        conn: &mut PgConnection,
        id: Uuid,
        token_nonce: &str,
        token_expires_at: DateTime<Utc>,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.engagement_ratings
               SET rating_value = NULL,
                   feedback = NULL,
                   consumed = false,
                   rated_on = NULL,
                   token_nonce = $2,
                   token_expires_at = $3
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(token_nonce)
        .bind(token_expires_at)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // ── publisher reply ──────────────────────────────────────────────────────

    /// The publisher-reply verb's write: the comment with the author and
    /// timestamp forced server-side (the trio is only ever written whole).
    pub async fn write_publisher_reply(
        conn: &mut PgConnection,
        id: Uuid,
        publisher_comment: &str,
        publisher_user_id: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"UPDATE engagement.engagement_ratings
               SET publisher_comment = $2,
                   publisher_user_id = $3,
                   publisher_replied_at = now()
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .bind(publisher_comment)
        .bind(publisher_user_id)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    // ── stats (the derived bands) ────────────────────────────────────────────

    /// Aggregate one target's consumed ratings: total, average, and the three
    /// derived bands (top >= 8, ok 5..=7, ko <= 4 — the upstream 4/3/1
    /// thresholds scaled x2 for the 1..10 scale). Unconsumed rows are
    /// invisible to every stats domain.
    pub async fn record_stats(
        conn: &mut PgConnection,
        rated_model: &str,
        rated_record_id: Uuid,
    ) -> Result<(i64, f64, i64, i64, i64), sqlx::Error> {
        sqlx::query_as::<_, (i64, f64, i64, i64, i64)>(
            r#"SELECT
                   count(*),
                   COALESCE(avg(rating_value), 0)::float8,
                   count(*) FILTER (WHERE rating_value >= 8),
                   count(*) FILTER (WHERE rating_value BETWEEN 5 AND 7),
                   count(*) FILTER (WHERE rating_value <= 4)
               FROM engagement.engagement_ratings
               WHERE rated_model = $1
                 AND rated_record_id = $2
                 AND consumed = true
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(rated_model)
        .bind(rated_record_id)
        .fetch_one(&mut *conn)
        .await
    }

    /// The parent rollup twin: aggregate consumed ratings over the
    /// denormalized parent pair (satisfaction across a child collection).
    pub async fn parent_stats(
        conn: &mut PgConnection,
        parent_rated_model: &str,
        parent_rated_record_id: Uuid,
    ) -> Result<(i64, f64, i64, i64, i64), sqlx::Error> {
        sqlx::query_as::<_, (i64, f64, i64, i64, i64)>(
            r#"SELECT
                   count(*),
                   COALESCE(avg(rating_value), 0)::float8,
                   count(*) FILTER (WHERE rating_value >= 8),
                   count(*) FILTER (WHERE rating_value BETWEEN 5 AND 7),
                   count(*) FILTER (WHERE rating_value <= 4)
               FROM engagement.engagement_ratings
               WHERE parent_rated_model = $1
                 AND parent_rated_record_id = $2
                 AND consumed = true
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(parent_rated_model)
        .bind(parent_rated_record_id)
        .fetch_one(&mut *conn)
        .await
    }
}

impl Default for RatingRepository {
    fn default() -> Self {
        Self::new()
    }
}
