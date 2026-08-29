//! `RatingWriteService` — the validated rating write path
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Family shape: a concrete service over the SQL held in
//! [`crate::infrastructure::persistence::rating_repository`]
//! (services orchestrate, repositories hold SQL), an error enum carrying
//! `code()`/`http_status()`, and one transaction per verb.
//!
//! What lives here, and why:
//!
//! - **The rated-type registry** — rated targets are REGISTERED entity
//!   names, never free text (the upstream `ir.model` lookup does not port).
//!   Each entry declares the parent model label used to denormalize the
//!   rollup pair at issue. The registry is code, composed at build time
//!   through the module builder.
//! - **Tier A capability tokens** — possession of the link is the auth, but
//!   a leaked row alone is inert: the public link carries
//!   `{id}.{nonce}.{exp}.{mac}` where the MAC is HMAC-SHA256 over
//!   `(id, nonce, grant, exp)` with a server secret, and `grant` binds the
//!   capability to what it may rate (`{rated_model}:{rated_record_id}`).
//!   The nonce alone is not the capability. Verification recomputes the MAC
//!   over the STORED row fields and compares in constant time
//!   (`hmac::Mac::verify_slice`).
//! - **Single-use submit** (rule R-R2) — one conditional UPDATE keyed on
//!   `consumed = false AND token_nonce = $ AND token_expires_at > now()`.
//!   Zero rows is the refusal: already-consumed, unknown token, and expired
//!   are indistinguishable (no oracle). The upstream re-submission /
//!   feedback-edit window does not port; the correction path is the
//!   issuer's reset.
//! - **The 1..10 scale** (rule R-R1, the recorded deviation from the forced
//!   {1,3,5}) — out-of-range is a typed 422 with a DB CHECK backstop. Bands
//!   are DERIVED read-side (top >= 8 / ok 5..=7 / ko <= 4), never stored.
//! - **Publisher reply** — the merchant-trio (comment, author, timestamp)
//!   is written only by the typed reply verb, which forces author and
//!   timestamp server-side.

use std::collections::HashMap;

use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use sqlx::PgPool;
use uuid::Uuid;

use crate::infrastructure::persistence::rating_repository::RatingRepository;

/// Default token lifetime at issue (30 days).
pub const DEFAULT_TOKEN_TTL_DAYS: i64 = 30;
/// Longest accepted rated-model name (the registry key grain).
pub const MAX_MODEL_LEN: usize = 120;
/// Longest accepted feedback comment.
pub const MAX_FEEDBACK_LEN: usize = 4000;
/// Longest accepted publisher comment.
pub const MAX_PUBLISHER_COMMENT_LEN: usize = 4000;
/// Longest accepted rater email (display-only label, never an auth input).
pub const MAX_RATER_EMAIL_LEN: usize = 320;
/// Lowest legal rating value (the 1..10 scale).
pub const RATING_MIN: i32 = 1;
/// Highest legal rating value.
pub const RATING_MAX: i32 = 10;

/// Env var holding the HMAC secret for rating tokens (a high-entropy string;
/// rotation invalidates every outstanding link — rotate via reset waves).
pub const RATING_TOKEN_SECRET_ENV: &str = "ENGAGEMENT_RATING_TOKEN_SECRET";

type HmacSha256 = Hmac<Sha256>;

// ─── error surface ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum RatingError {
    #[error("rated model is not registered: {0}")]
    RatedTypeUnknown(String),
    #[error("rated model name exceeds {MAX_MODEL_LEN} characters")]
    RatedModelTooLong,
    #[error("rating value must be between {RATING_MIN} and {RATING_MAX}")]
    RatingOutOfRange,
    #[error("feedback exceeds {MAX_FEEDBACK_LEN} characters")]
    FeedbackTooLong,
    #[error("publisher comment exceeds {MAX_PUBLISHER_COMMENT_LEN} characters")]
    PublisherCommentTooLong,
    #[error("rater email exceeds {MAX_RATER_EMAIL_LEN} characters")]
    RaterEmailTooLong,
    #[error("rating request not found")]
    RatingNotFound,
    #[error("the rating token is not submittable (unknown, already used, or expired)")]
    NotSubmittable,
    #[error("the token link is malformed")]
    TokenMalformed,
    #[error("no token secret is configured (set {RATING_TOKEN_SECRET_ENV} or pass one at composition)")]
    SecretNotConfigured,
    #[error("internal error: {0}")]
    Internal(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl RatingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::RatedTypeUnknown(_) => "rating_rated_type_unknown",
            Self::RatedModelTooLong => "rating_rated_model_too_long",
            Self::RatingOutOfRange => "rating_out_of_range",
            Self::FeedbackTooLong => "rating_feedback_too_long",
            Self::PublisherCommentTooLong => "rating_publisher_comment_too_long",
            Self::RaterEmailTooLong => "rating_rater_email_too_long",
            Self::RatingNotFound => "rating_not_found",
            Self::NotSubmittable => "rating_not_submittable",
            Self::TokenMalformed => "rating_token_malformed",
            Self::SecretNotConfigured => "rating_secret_not_configured",
            Self::Internal(_) => "internal_error",
            Self::Db(_) => "database_error",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::RatingNotFound => 404,
            Self::NotSubmittable => 409,
            Self::Internal(_) | Self::Db(_) | Self::SecretNotConfigured => 500,
            _ => 422,
        }
    }
}

// ─── the rated-type registry ──────────────────────────────────────────────────

/// One registered rated type: the entity name a rating may target, plus the
/// declared parent model label used to denormalize the rollup pair at issue
/// (the parent RECORD id is supplied by the issuer — the rated record lives
/// in another module's schema and is never read from here).
#[derive(Debug, Clone)]
pub struct RatedType {
    pub model: String,
    pub parent_model: Option<String>,
}

/// Registered rated targets — composition-time code, never free text. An
/// empty registry refuses every issue (fail closed: an unregistered model
/// name is a bug in composition, not a data problem).
#[derive(Debug, Clone, Default)]
pub struct RatedTypeRegistry {
    entries: HashMap<String, RatedType>,
}

impl RatedTypeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a rated type; `parent_model` declares the rollup grain.
    pub fn register(&mut self, model: &str, parent_model: Option<&str>) {
        self.entries.insert(
            model.to_string(),
            RatedType {
                model: model.to_string(),
                parent_model: parent_model.map(String::from),
            },
        );
    }

    pub fn get(&self, model: &str) -> Option<&RatedType> {
        self.entries.get(model)
    }
}

// ─── inputs / outputs ─────────────────────────────────────────────────────────

/// The minted (or reused) rating request plus the public capability link.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingTokenView {
    pub id: Uuid,
    /// `{id}.{nonce}.{exp}.{mac}` — the string to embed in the outbound link.
    pub link: String,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    /// True when this call MINTED the request; false when the live
    /// unconsumed row for this identified rater was reused (idempotent
    /// issuance). Anonymous (NULL-rater) requests always mint.
    pub created: bool,
}

/// Aggregate view of one target's consumed ratings — bands are derived
/// read-side (top >= 8 / ok 5..=7 / ko <= 4; satisfaction = top share).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RatingStats {
    pub total: i64,
    pub average: f64,
    pub top: i64,
    pub ok: i64,
    pub ko: i64,
    /// top/total x 100, rounded — 0 when nothing is consumed (a plain zero,
    /// never the upstream -1 sentinel).
    pub percentage_satisfaction: i64,
}

impl RatingStats {
    fn build(total: i64, average: f64, top: i64, ok: i64, ko: i64) -> Self {
        let satisfaction = if total > 0 {
            (top as f64 / total as f64 * 100.0).round() as i64
        } else {
            0
        };
        Self { total, average, top, ok, ko, percentage_satisfaction: satisfaction }
    }
}

// ─── token machinery ──────────────────────────────────────────────────────────

/// A parsed capability link.
#[derive(Debug)]
struct TokenParts {
    id: Uuid,
    nonce: String,
    exp: i64,
    mac: String,
}

/// Mint a fresh 128-bit hex selector.
fn mint_nonce() -> String {
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The MAC input: `(id, nonce, grant, exp)` where the grant binds the
/// capability to the target it may rate.
fn mac_input(id: &Uuid, nonce: &str, grant: &str, exp: i64) -> String {
    format!("{id}.{nonce}.{grant}.{exp}")
}

/// HMAC-SHA256 over the input, hex-encoded.
fn compute_mac(secret: &[u8], input: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    let out = mac.finalize().into_bytes();
    out.iter().map(|b| format!("{b:02x}")).collect()
}

/// Constant-time MAC comparison (`hmac::Mac::verify_slice` — designed to be
/// constant-time over the tag comparison).
fn verify_mac(secret: &[u8], input: &str, provided: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(input.as_bytes());
    match hex_decode(provided) {
        Some(bytes) => mac.verify_slice(&bytes).is_ok(),
        None => false,
    }
}

/// Minimal hex decode (no external dependency; returns None on any non-hex
/// byte — a malformed tag simply fails verification).
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 2);
    let bytes = s.as_bytes();
    for pair in bytes.chunks(2) {
        let hi = (pair[0] as char).to_digit(16)?;
        let lo = (pair[1] as char).to_digit(16)?;
        out.push((hi * 16 + lo) as u8);
    }
    Some(out)
}

/// Parse `{id}.{nonce}.{exp}.{mac}` — exactly four dot-separated segments
/// (the uuid and the hex mac carry no dots of their own).
fn parse_link(link: &str) -> Option<TokenParts> {
    let mut segments = link.split('.');
    let id = segments.next()?.parse().ok()?;
    let nonce = segments.next()?.to_string();
    let exp = segments.next()?.parse().ok()?;
    let mac = segments.next()?.to_string();
    if segments.next().is_some() {
        return None;
    }
    Some(TokenParts { id, nonce, exp, mac })
}

// ─── the service ──────────────────────────────────────────────────────────────

pub struct RatingWriteService {
    pool: PgPool,
    secret: Vec<u8>,
    rated_types: RatedTypeRegistry,
}

impl RatingWriteService {
    /// Construct with the HMAC secret from the environment
    /// ([`RATING_TOKEN_SECRET_ENV`]). Issuing and verifying both refuse
    /// (typed 500) when the variable is unset — there is no silent
    /// zero-secret fallback.
    pub fn new(pool: PgPool) -> Self {
        let secret = std::env::var(RATING_TOKEN_SECRET_ENV).unwrap_or_default();
        Self {
            pool,
            secret: secret.into_bytes(),
            rated_types: RatedTypeRegistry::new(),
        }
    }

    /// Construct with an explicit secret and registry (composition and
    /// tests; the env var path is [`Self::new`]).
    pub fn with_config(pool: PgPool, secret: &[u8], rated_types: RatedTypeRegistry) -> Self {
        Self { pool, secret: secret.to_vec(), rated_types }
    }

    /// Register a rated type at composition time (builder-style, before the
    /// service is shared).
    pub fn register_rated_type(&mut self, model: &str, parent_model: Option<&str>) -> &mut Self {
        self.rated_types.register(model, parent_model);
        self
    }

    fn require_secret(&self) -> Result<&[u8], RatingError> {
        if self.secret.is_empty() {
            return Err(RatingError::SecretNotConfigured);
        }
        Ok(&self.secret)
    }

    fn resolve_rated_type(&self, rated_model: &str) -> Result<&RatedType, RatingError> {
        if rated_model.chars().count() > MAX_MODEL_LEN {
            return Err(RatingError::RatedModelTooLong);
        }
        self.rated_types
            .get(rated_model)
            .ok_or_else(|| RatingError::RatedTypeUnknown(rated_model.to_string()))
    }

    fn clean_rater_email(raw: Option<&str>) -> Result<Option<String>, RatingError> {
        match raw {
            Some(email) => {
                let trimmed = email.trim();
                if trimmed.is_empty() {
                    return Ok(None);
                }
                if trimmed.chars().count() > MAX_RATER_EMAIL_LEN {
                    return Err(RatingError::RaterEmailTooLong);
                }
                if trimmed.chars().any(|c| c.is_control()) {
                    return Err(RatingError::RaterEmailTooLong);
                }
                Ok(Some(trimmed.to_string()))
            }
            None => Ok(None),
        }
    }

    fn grant_for(rated_model: &str, rated_record_id: Uuid) -> String {
        format!("{rated_model}:{rated_record_id}")
    }

    fn build_link(
        &self,
        id: Uuid,
        nonce: &str,
        rated_model: &str,
        rated_record_id: Uuid,
        exp_unix: i64,
    ) -> Result<String, RatingError> {
        let secret = self.require_secret()?;
        let input = mac_input(&id, nonce, &Self::grant_for(rated_model, rated_record_id), exp_unix);
        let mac = compute_mac(secret, &input);
        Ok(format!("{id}.{nonce}.{exp_unix}.{mac}"))
    }

    // ── issue ────────────────────────────────────────────────────────────────

    /// Issue (or reuse) a rating request against a registered rated type.
    ///
    /// Idempotent for identified raters (rule R-R3): the live unconsumed row
    /// for `(rated_model, rated_record_id, rater)` is reused as-is — its
    /// nonce stays, its link is re-derived. Anonymous (`rater = None`)
    /// requests always mint a fresh row (the idempotency index only covers
    /// identified raters by design). `parent_rated_record_id` denormalizes
    /// the rollup pair from the registry's declared parent model.
    #[allow(clippy::too_many_arguments)]
    pub async fn issue_rating(
        &self,
        rated_model: &str,
        rated_record_id: Uuid,
        rater_user_id: Option<Uuid>,
        rater_email: Option<&str>,
        rated_user_id: Option<Uuid>,
        parent_rated_record_id: Option<Uuid>,
        ttl_days: Option<i64>,
    ) -> Result<RatingTokenView, RatingError> {
        let entry = self.resolve_rated_type(rated_model)?;
        let rater_email = Self::clean_rater_email(rater_email)?;
        let ttl = ttl_days.unwrap_or(DEFAULT_TOKEN_TTL_DAYS);
        let _ = self.require_secret()?;

        let mut tx = self.pool.begin().await?;

        // Idempotent arm: reuse the identified rater's live unconsumed row.
        if let Some(rater) = rater_user_id {
            if let Some(existing) = RatingRepository::find_live_unconsumed(
                &mut tx,
                rated_model,
                rated_record_id,
                rater,
            )
            .await?
            {
                let exp = existing.token_expires_at.timestamp();
                let link = self.build_link(
                    existing.id,
                    &existing.token_nonce,
                    rated_model,
                    rated_record_id,
                    exp,
                )?;
                tx.commit().await?;
                return Ok(RatingTokenView {
                    id: existing.id,
                    link,
                    expires_at: existing.token_expires_at,
                    created: false,
                });
            }
        }

        // The parent pair denormalizes from the registry: a parent record id
        // only pairs with a declared parent model.
        let parent_model_label =
            entry.parent_model.as_deref().filter(|_| parent_rated_record_id.is_some());
        let nonce = mint_nonce();
        let expires_at = Utc::now() + Duration::days(ttl);
        let id = Uuid::new_v4();

        let row = RatingRepository::insert_rating(
            &mut tx,
            id,
            rated_model,
            rated_record_id,
            parent_model_label,
            parent_rated_record_id,
            rated_user_id,
            rater_user_id,
            rater_email.as_deref(),
            &nonce,
            expires_at,
            rater_user_id,
        )
        .await;

        let row = match row {
            Ok(row) => row,
            // A concurrent issuance for the same identified rater won the
            // partial-unique race — converge on the winner (the index's
            // error code is the unique-violation 23505 family).
            Err(sqlx::Error::Database(db)) if db.is_unique_violation() => {
                let rater = rater_user_id.expect("unique violation implies identified rater");
                let existing = RatingRepository::find_live_unconsumed(
                    &mut tx,
                    rated_model,
                    rated_record_id,
                    rater,
                )
                .await?
                .ok_or_else(|| {
                    RatingError::Internal("unique violation with no live row to converge on".into())
                })?;
                existing
            }
            Err(e) => return Err(e.into()),
        };

        let exp = row.token_expires_at.timestamp();
        let link = self.build_link(row.id, &row.token_nonce, rated_model, rated_record_id, exp)?;
        tx.commit().await?;
        Ok(RatingTokenView { id: row.id, link, expires_at: row.token_expires_at, created: true })
    }

    // ── submit (public, single-use) ─────────────────────────────────────────

    /// Submit an answer through a capability link — ONE conditional UPDATE,
    /// single-use (rule R-R2). The MAC is verified first, in constant time,
    /// against the STORED row's own fields; every refusal after that is the
    /// shared `NotSubmittable` shape (already-consumed / unknown / expired /
    /// bad token are indistinguishable — no oracle for a link forger).
    /// Returns the consumed row's id.
    pub async fn submit_rating(
        &self,
        link: &str,
        rating_value: i32,
        feedback: Option<&str>,
    ) -> Result<Uuid, RatingError> {
        if !(RATING_MIN..=RATING_MAX).contains(&rating_value) {
            return Err(RatingError::RatingOutOfRange);
        }
        let feedback = match feedback {
            Some(f) => {
                let trimmed = f.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    if trimmed.chars().count() > MAX_FEEDBACK_LEN {
                        return Err(RatingError::FeedbackTooLong);
                    }
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        let secret = self.require_secret()?;
        let parts = parse_link(link).ok_or(RatingError::NotSubmittable)?;

        let mut tx = self.pool.begin().await?;

        // Load the row the selector points at; recompute the MAC over the
        // STORED fields so a forged nonce/exp cannot ride a valid id.
        let row = RatingRepository::find_by_id(&mut tx, parts.id)
            .await?
            .ok_or(RatingError::NotSubmittable)?;
        let grant = Self::grant_for(&row.rated_model, row.rated_record_id);
        let expected_input =
            mac_input(&row.id, &row.token_nonce, &grant, row.token_expires_at.timestamp());
        if !verify_mac(secret, &expected_input, &parts.mac)
            || parts.nonce != row.token_nonce
            || parts.exp != row.token_expires_at.timestamp()
        {
            // The verify-failure observable (throttle-adjacent audit event);
            // the response stays the shared refusal shape.
            tracing::warn!(rating = %row.id, "rating_token_verify_failed");
            tx.commit().await?;
            return Err(RatingError::NotSubmittable);
        }

        let submitted = RatingRepository::conditional_submit(
            &mut tx,
            row.id,
            &row.token_nonce,
            rating_value,
            feedback.as_deref(),
        )
        .await?;

        tx.commit().await?;
        if !submitted {
            return Err(RatingError::NotSubmittable);
        }
        Ok(row.id)
    }

    // ── rotate / reset (the issuer's correction paths) ──────────────────────

    /// Re-arm the capability with a fresh nonce + expiry (extends a stale
    /// link; the old link dies with the old nonce).
    pub async fn rotate_token(
        &self,
        rating_id: Uuid,
        ttl_days: Option<i64>,
    ) -> Result<RatingTokenView, RatingError> {
        self.reissue(rating_id, ttl_days, false).await
    }

    /// Reset for a reuse wave: clear the answer and re-arm with a NEW nonce
    /// + expiry (the correction path that replaces the upstream
    /// re-submission window).
    pub async fn reset_rating(
        &self,
        rating_id: Uuid,
        ttl_days: Option<i64>,
    ) -> Result<RatingTokenView, RatingError> {
        self.reissue(rating_id, ttl_days, true).await
    }

    async fn reissue(
        &self,
        rating_id: Uuid,
        ttl_days: Option<i64>,
        clear_answer: bool,
    ) -> Result<RatingTokenView, RatingError> {
        let _ = self.require_secret()?;
        let mut tx = self.pool.begin().await?;
        let row = RatingRepository::find_by_id(&mut tx, rating_id)
            .await?
            .ok_or(RatingError::RatingNotFound)?;

        let nonce = mint_nonce();
        let expires_at = Utc::now() + Duration::days(ttl_days.unwrap_or(DEFAULT_TOKEN_TTL_DAYS));
        let updated = if clear_answer {
            RatingRepository::reset_rating(&mut tx, rating_id, &nonce, expires_at).await?
        } else {
            RatingRepository::rotate_token(&mut tx, rating_id, &nonce, expires_at).await?
        };
        if !updated {
            tx.commit().await?;
            return Err(RatingError::RatingNotFound);
        }
        let exp = expires_at.timestamp();
        let link = self.build_link(rating_id, &nonce, &row.rated_model, row.rated_record_id, exp)?;
        tx.commit().await?;
        Ok(RatingTokenView { id: rating_id, link, expires_at, created: true })
    }

    // ── publisher reply ──────────────────────────────────────────────────────

    /// Write the publisher's public reply — the merchant trio is written
    /// only here, author and timestamp forced server-side.
    pub async fn publisher_reply(
        &self,
        rating_id: Uuid,
        comment: &str,
        publisher_user_id: Uuid,
    ) -> Result<(), RatingError> {
        let trimmed = comment.trim();
        if trimmed.is_empty() {
            return Err(RatingError::PublisherCommentTooLong);
        }
        if trimmed.chars().count() > MAX_PUBLISHER_COMMENT_LEN {
            return Err(RatingError::PublisherCommentTooLong);
        }
        let mut tx = self.pool.begin().await?;
        let wrote = RatingRepository::write_publisher_reply(
            &mut tx,
            rating_id,
            trimmed,
            publisher_user_id,
        )
        .await?;
        tx.commit().await?;
        if !wrote {
            return Err(RatingError::RatingNotFound);
        }
        Ok(())
    }

    // ── reads ────────────────────────────────────────────────────────────────

    /// One rated record's aggregate over consumed ratings (bands derived
    /// read-side; unconsumed rows are invisible).
    pub async fn record_stats(
        &self,
        rated_model: &str,
        rated_record_id: Uuid,
    ) -> Result<RatingStats, RatingError> {
        let mut conn = self.pool.acquire().await?;
        let (total, average, top, ok, ko) =
            RatingRepository::record_stats(&mut conn, rated_model, rated_record_id).await?;
        Ok(RatingStats::build(total, average, top, ok, ko))
    }

    /// The parent rollup twin (satisfaction across a child collection).
    pub async fn parent_stats(
        &self,
        parent_rated_model: &str,
        parent_rated_record_id: Uuid,
    ) -> Result<RatingStats, RatingError> {
        let mut conn = self.pool.acquire().await?;
        let (total, average, top, ok, ko) =
            RatingRepository::parent_stats(&mut conn, parent_rated_model, parent_rated_record_id)
                .await?;
        Ok(RatingStats::build(total, average, top, ok, ko))
    }
}
