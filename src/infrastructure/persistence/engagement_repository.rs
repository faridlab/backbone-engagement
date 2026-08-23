//! `EngagementRepository` — the hand-written SQL behind the attribution and
//! tracker write paths (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Module rule: services orchestrate, repositories hold SQL. Runtime queries
//! — no `.sqlx` macros, so no compile-time cache is needed. Every method
//! takes a connection inside the caller's transaction; every read filters on
//! `metadata->>'deleted_at' IS NULL` (the soft-delete convention) except
//! where noted. The module is unfenced (ADR-0014 posture 4): there is no
//! company predicate to carry — tenant isolation is the database boundary.

use chrono::NaiveDate;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::domain::entity::EngagementLinkTracker;

/// Hand-written engagement SQL. Services orchestrate; this holds SQL.
pub struct EngagementRepository;

impl EngagementRepository {
    pub fn new() -> Self {
        Self
    }

    // ── masters: the find-or-create grain ────────────────────────────────────

    /// Case-insensitive live-medium lookup by exact name (upstream's `=ilike`
    /// grain — equality, never a wildcard match).
    pub async fn find_medium_by_name(
        conn: &mut PgConnection,
        name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM engagement.engagement_media
               WHERE lower(name) = lower($1)
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Insert a medium row. `name` must already be validated + unique.
    pub async fn insert_medium(
        conn: &mut PgConnection,
        id: Uuid,
        name: &str,
        created_by: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO engagement.engagement_media (id, name, metadata)
               VALUES ($1, $2, jsonb_build_object(
                   'created_by', $3::text,
                   'created_at', to_jsonb(now())
               ))"#,
        )
        .bind(id)
        .bind(name)
        .bind(created_by.map(|u| u.to_string()))
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Case-insensitive live-source lookup by exact name.
    pub async fn find_source_by_name(
        conn: &mut PgConnection,
        name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM engagement.engagement_sources
               WHERE lower(name) = lower($1)
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Insert a source row.
    pub async fn insert_source(
        conn: &mut PgConnection,
        id: Uuid,
        name: &str,
        created_by: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO engagement.engagement_sources (id, name, metadata)
               VALUES ($1, $2, jsonb_build_object(
                   'created_by', $3::text,
                   'created_at', to_jsonb(now())
               ))"#,
        )
        .bind(id)
        .bind(name)
        .bind(created_by.map(|u| u.to_string()))
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Case-insensitive live-campaign lookup by exact identifier (`name`, the
    /// URL-facing key — not the human title).
    pub async fn find_campaign_by_name(
        conn: &mut PgConnection,
        name: &str,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT id FROM engagement.engagement_campaigns
               WHERE lower(name) = lower($1)
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(name)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Insert a campaign row.
    pub async fn insert_campaign(
        conn: &mut PgConnection,
        id: Uuid,
        name: &str,
        title: &str,
        is_auto_campaign: bool,
        user_id: Option<Uuid>,
        created_by: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"INSERT INTO engagement.engagement_campaigns
                   (id, name, title, active, user_id, is_auto_campaign, metadata)
               VALUES ($1, $2, $3, true, $4, $5, jsonb_build_object(
                   'created_by', $6::text,
                   'created_at', to_jsonb(now())
               ))"#,
        )
        .bind(id)
        .bind(name)
        .bind(title)
        .bind(user_id)
        .bind(is_auto_campaign)
        .bind(created_by.map(|u| u.to_string()))
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// The unique-name engine's probe: does this identifier exist among live
    /// campaigns (case-insensitive)?
    pub async fn campaign_name_taken(
        conn: &mut PgConnection,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        Ok(Self::find_campaign_by_name(conn, name).await?.is_some())
    }

    // ── masters: redirect-time name resolution ──────────────────────────────

    /// Resolve the display names of the attribution masters a tracker points
    /// at (campaign, medium, source) — the values injected as utm_* params on
    /// redirect. Live rows only; a soft-deleted master contributes nothing.
    pub async fn master_names(
        conn: &mut PgConnection,
        campaign_id: Option<Uuid>,
        medium_id: Option<Uuid>,
        source_id: Option<Uuid>,
    ) -> Result<(Option<String>, Option<String>, Option<String>), sqlx::Error> {
        let campaign = match campaign_id {
            Some(id) => {
                sqlx::query_scalar::<_, String>(
                    r#"SELECT name FROM engagement.engagement_campaigns
                       WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
            }
            None => None,
        };
        let medium = match medium_id {
            Some(id) => {
                sqlx::query_scalar::<_, String>(
                    r#"SELECT name FROM engagement.engagement_media
                       WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
            }
            None => None,
        };
        let source = match source_id {
            Some(id) => {
                sqlx::query_scalar::<_, String>(
                    r#"SELECT name FROM engagement.engagement_sources
                       WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
                )
                .bind(id)
                .fetch_optional(&mut *conn)
                .await?
            }
            None => None,
        };
        Ok((campaign, medium, source))
    }

    // ── trackers ─────────────────────────────────────────────────────────────

    /// Find the live tracker matching the uniqueness 5-tuple with upstream's
    /// NULL semantics (an absent attribution matches an absent one —
    /// `IS NOT DISTINCT FROM`; a plain SQL UNIQUE would treat them as
    /// distinct, which is exactly why upstream keeps this check in code).
    pub async fn find_tracker_by_tuple(
        conn: &mut PgConnection,
        url: &str,
        campaign_id: Option<Uuid>,
        medium_id: Option<Uuid>,
        source_id: Option<Uuid>,
        label: &str,
    ) -> Result<Option<EngagementLinkTracker>, sqlx::Error> {
        sqlx::query_as::<_, EngagementLinkTracker>(
            r#"SELECT * FROM engagement.engagement_link_trackers
               WHERE url = $1
                 AND campaign_id IS NOT DISTINCT FROM $2
                 AND medium_id  IS NOT DISTINCT FROM $3
                 AND source_id  IS NOT DISTINCT FROM $4
                 AND label = $5
                 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(url)
        .bind(campaign_id)
        .bind(medium_id)
        .bind(source_id)
        .bind(label)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Does any live tracker already hold this short code?
    pub async fn code_taken(conn: &mut PgConnection, code: &str) -> Result<bool, sqlx::Error> {
        let hit: Option<i64> = sqlx::query_scalar(
            r#"SELECT 1 FROM engagement.engagement_link_trackers
               WHERE code = $1 AND (metadata->>'deleted_at') IS NULL
               LIMIT 1"#,
        )
        .bind(code)
        .fetch_optional(&mut *conn)
        .await?;
        Ok(hit.is_some())
    }

    /// Insert a tracker row. The URL must have been validated (absolute
    /// http(s)) by the service; `code` must be free.
    pub async fn insert_tracker(
        conn: &mut PgConnection,
        id: Uuid,
        url: &str,
        code: &str,
        title: Option<&str>,
        label: &str,
        campaign_id: Option<Uuid>,
        medium_id: Option<Uuid>,
        source_id: Option<Uuid>,
        created_by: Option<Uuid>,
    ) -> Result<EngagementLinkTracker, sqlx::Error> {
        sqlx::query_as::<_, EngagementLinkTracker>(
            r#"INSERT INTO engagement.engagement_link_trackers
                   (id, url, code, title, label, campaign_id, medium_id, source_id, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, jsonb_build_object(
                   'created_by', $9::text,
                   'created_at', to_jsonb(now())
               ))
               RETURNING *"#,
        )
        .bind(id)
        .bind(url)
        .bind(code)
        .bind(title)
        .bind(label)
        .bind(campaign_id)
        .bind(medium_id)
        .bind(source_id)
        .bind(created_by.map(|u| u.to_string()))
        .fetch_one(&mut *conn)
        .await
    }

    /// Resolve a live tracker by its short code (the /r/<code> lookup).
    pub async fn find_tracker_by_code(
        conn: &mut PgConnection,
        code: &str,
    ) -> Result<Option<EngagementLinkTracker>, sqlx::Error> {
        sqlx::query_as::<_, EngagementLinkTracker>(
            r#"SELECT * FROM engagement.engagement_link_trackers
               WHERE code = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(code)
        .fetch_optional(&mut *conn)
        .await
    }

    /// Resolve a live tracker by id (the detail read).
    pub async fn find_tracker_by_id(
        conn: &mut PgConnection,
        id: Uuid,
    ) -> Result<Option<EngagementLinkTracker>, sqlx::Error> {
        sqlx::query_as::<_, EngagementLinkTracker>(
            r#"SELECT * FROM engagement.engagement_link_trackers
               WHERE id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
    }

    /// A tracker's current click count (the projection upstream stores on the
    /// row; here it is always read fresh from the ledger).
    pub async fn click_count(conn: &mut PgConnection, link_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT count(*) FROM engagement.engagement_link_tracker_clicks
               WHERE link_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(link_id)
        .fetch_one(&mut *conn)
        .await
    }

    // ── clicks ───────────────────────────────────────────────────────────────

    /// Mint a click row idempotently: `ON CONFLICT DO NOTHING` rides the
    /// unique index on `dedup_key`, so a replayed hit converges instead of
    /// minting. True when a row was actually inserted (a first-of-day hit).
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_click(
        conn: &mut PgConnection,
        id: Uuid,
        link_id: Uuid,
        campaign_id: Option<Uuid>,
        ip: Option<&str>,
        country_code: Option<&str>,
        click_day: NaiveDate,
        dedup_key: &str,
    ) -> Result<bool, sqlx::Error> {
        let res = sqlx::query(
            r#"INSERT INTO engagement.engagement_link_tracker_clicks
                   (id, link_id, campaign_id, ip, country_code, click_day, dedup_key, metadata)
               VALUES ($1, $2, $3, $4, $5, $6, $7, jsonb_build_object('created_at', to_jsonb(now())))
               ON CONFLICT DO NOTHING"#,
        )
        .bind(id)
        .bind(link_id)
        .bind(campaign_id)
        .bind(ip)
        .bind(country_code)
        .bind(click_day)
        .bind(dedup_key)
        .execute(&mut *conn)
        .await?;
        Ok(res.rows_affected() > 0)
    }

    /// Total deduped clicks recorded against one campaign (reads the clicks'
    /// own denormalized campaign, not the tracker's current attribution).
    pub async fn campaign_click_total(
        conn: &mut PgConnection,
        campaign_id: Uuid,
    ) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(
            r#"SELECT count(*) FROM engagement.engagement_link_tracker_clicks
               WHERE campaign_id = $1 AND (metadata->>'deleted_at') IS NULL"#,
        )
        .bind(campaign_id)
        .fetch_one(&mut *conn)
        .await
    }
}

impl Default for EngagementRepository {
    fn default() -> Self {
        Self::new()
    }
}
