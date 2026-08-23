//! `EngagementWriteService` — the validated engagement write path
//! (hand-authored, user-owned; see `metaphor.codegen.yaml`).
//!
//! Family shape: a concrete service over the SQL held in
//! [`crate::infrastructure::persistence::engagement_repository`]
//! (services orchestrate, repositories hold SQL), an error enum carrying
//! `code()`/`http_status()`, and one transaction per verb.
//!
//! What lives here, and why:
//!
//! - **Attribution find-or-create** (`resolve_attribution`) — the port of
//!   utm's cookie ferry. Upstream plants `odoo_utm_*` cookies from ANY
//!   public request and later find-or-CREATES master data from those raw
//!   browser strings, unvalidated. The port keeps the seam but moves it
//!   behind validation: values are trimmed, length-capped, and control-char
//!   rejected BEFORE anything is minted, and cookie planting itself is the
//!   host webapp's concern. Mints carry the auto marker
//!   (`is_auto_campaign`) so authored and minted campaigns stay separable.
//! - **Campaign unique-name engine** — identifiers mint from the title with
//!   `Name [N]` counters filling the first free slot (upstream's
//!   disambiguation engine, EN-23).
//! - **Tracker minting** (`create_tracker`) — idempotent on the 5-tuple
//!   (url, campaign, medium, source, label) with upstream's NULL semantics;
//!   the target must be ABSOLUTE http(s) (the no-open-redirect rule, applied
//!   at create AND re-checked at redirect); the short code is random
//!   [A-Za-z0-9] from length 3 with the length-growing retry loop; NO
//!   server-side title fetch is ever performed (the og:title egress is
//!   closed by decision — titles arrive from the caller, previews render
//!   client-side).
//! - **Redirect resolution** (`resolve_redirect`) — resolves a code, builds
//!   the target with the tracker's utm_* params injected, and mints a click
//!   IDEMPOTENTLY: the (link, ip, UTC-day) dedup key makes replays converge
//!   (the ADR-0019 action_link idempotency contract), and a per-tracker cap
//!   bounds the ledger. Minting failure never blocks the redirect except for
//!   a corrupt stored target.

use chrono::Utc;
use rand::Rng;
use sqlx::PgPool;
use uuid::Uuid;

use crate::domain::entity::EngagementLinkTracker;
use crate::infrastructure::persistence::engagement_repository::EngagementRepository;

/// The shortest minted short code (upstream's `LINK_TRACKER_MIN_CODE_LENGTH`).
pub const MIN_CODE_LENGTH: usize = 3;
/// The longest minted short code — the retry loop's ceiling (the code space
/// at length 12 is ~3e21; exhausting it means something is already wrong).
pub const MAX_CODE_LENGTH: usize = 12;
/// Hard ceiling on counted clicks per tracker (a storage-exhaustion
/// backstop; the redirect itself never depends on minting succeeding).
pub const MAX_CLICKS_PER_TRACKER: i64 = 100_000;
/// Longest accepted attribution name / campaign title or identifier.
pub const MAX_NAME_LEN: usize = 120;
/// Longest accepted tracker label (upstream's extractor cap).
pub const MAX_LABEL_LEN: usize = 40;
/// Longest accepted tracker title.
pub const MAX_TITLE_LEN: usize = 300;
/// Longest accepted target URL.
pub const MAX_URL_LEN: usize = 2000;

// ─── error surface ────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum EngagementError {
    #[error("attribution value is empty after trimming")]
    EmptyAttributionValue,
    #[error("attribution value exceeds {MAX_NAME_LEN} characters")]
    AttributionValueTooLong,
    #[error("attribution value contains control characters")]
    AttributionValueControlChars,
    #[error("target URL must be absolute http(s) with a host")]
    NotAbsoluteHttpUrl,
    #[error("target URL exceeds {MAX_URL_LEN} characters")]
    UrlTooLong,
    #[error("tracker label exceeds {MAX_LABEL_LEN} characters")]
    LabelTooLong,
    #[error("tracker title exceeds {MAX_TITLE_LEN} characters")]
    TitleTooLong,
    #[error("no free campaign identifier could be minted from this title")]
    CampaignNameExhausted,
    #[error("campaign identifier is already taken by a live campaign")]
    CampaignNameTaken,
    #[error("the short-code space is exhausted")]
    CodeSpaceExhausted,
    #[error("tracker not found")]
    TrackerNotFound,
    #[error("the stored target URL failed validation — refusing to redirect")]
    StoredTargetInvalid,
    #[error("internal error: {0}")]
    Internal(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

impl EngagementError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::EmptyAttributionValue => "engagement_attribution_empty",
            Self::AttributionValueTooLong => "engagement_attribution_too_long",
            Self::AttributionValueControlChars => "engagement_attribution_control_chars",
            Self::NotAbsoluteHttpUrl => "engagement_not_absolute_http_url",
            Self::UrlTooLong => "engagement_url_too_long",
            Self::LabelTooLong => "engagement_label_too_long",
            Self::TitleTooLong => "engagement_title_too_long",
            Self::CampaignNameExhausted => "engagement_campaign_name_exhausted",
            Self::CampaignNameTaken => "engagement_campaign_name_taken",
            Self::CodeSpaceExhausted => "engagement_code_space_exhausted",
            Self::TrackerNotFound => "engagement_tracker_not_found",
            Self::StoredTargetInvalid => "engagement_stored_target_invalid",
            Self::Internal(_) => "internal_error",
            Self::Db(_) => "database_error",
        }
    }

    pub fn http_status(&self) -> u16 {
        match self {
            Self::TrackerNotFound => 404,
            Self::Internal(_) | Self::Db(_) | Self::StoredTargetInvalid
            | Self::CodeSpaceExhausted => 500,
            _ => 422,
        }
    }
}

// ─── inputs / outputs ─────────────────────────────────────────────────────────

/// Raw attribution strings as captured from the visitor (URL params / the
/// cookies the host webapp plants). `None` simply drops out of attribution.
#[derive(Debug, Clone, Default)]
pub struct AttributionInput {
    pub campaign: Option<String>,
    pub source: Option<String>,
    pub medium: Option<String>,
}

/// The resolved attribution triple: existing master ids, with unknown names
/// minted (find-or-create, the case-insensitive name grain).
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttributionRefs {
    pub campaign_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub medium_id: Option<Uuid>,
}

/// A minted-or-found tracker plus its live click count.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerView {
    pub id: Uuid,
    pub url: String,
    pub code: String,
    pub title: Option<String>,
    pub label: String,
    pub campaign_id: Option<Uuid>,
    pub medium_id: Option<Uuid>,
    pub source_id: Option<Uuid>,
    pub click_count: i64,
    /// True when this call MINTED the tracker; false when the 5-tuple
    /// already had one (the idempotent arm of search-or-create).
    pub created: bool,
}

/// The redirect target the public route sends the visitor to.
#[derive(Debug)]
pub struct RedirectTarget {
    pub url: String,
}

// ─── validation helpers ───────────────────────────────────────────────────────

/// Trim + validate a master-name-shaped value BEFORE any find-or-create runs
/// (the port-time hardening of the cookie ferry: attacker-controlled strings
/// never reach the masters unvalidated).
fn clean_attribution_value(raw: &str) -> Result<String, EngagementError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(EngagementError::EmptyAttributionValue);
    }
    if trimmed.chars().count() > MAX_NAME_LEN {
        return Err(EngagementError::AttributionValueTooLong);
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(EngagementError::AttributionValueControlChars);
    }
    Ok(trimmed.to_string())
}

/// An absolute http(s) URL with a non-empty host and no whitespace — the
/// create-time AND redirect-time no-open-redirect gate. Everything else
/// (javascript:, data:, relative paths, scheme-less //host) is refused.
fn validate_absolute_http_url(raw: &str) -> Result<String, EngagementError> {
    if raw.chars().count() > MAX_URL_LEN {
        return Err(EngagementError::UrlTooLong);
    }
    if raw.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(EngagementError::NotAbsoluteHttpUrl);
    }
    let (scheme, rest) = match raw.split_once("://") {
        Some(split) => split,
        None => return Err(EngagementError::NotAbsoluteHttpUrl),
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return Err(EngagementError::NotAbsoluteHttpUrl);
    }
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // A host must exist and carry at least one alphanum (rejects "http://:"
    // and userinfo-only shapes like "http://user@").
    if host.is_empty() || !host.chars().any(|c| c.is_ascii_alphanumeric()) {
        return Err(EngagementError::NotAbsoluteHttpUrl);
    }
    Ok(raw.to_string())
}

/// Percent-encode a query-parameter VALUE (unreserved characters pass;
/// everything else — space, `&`, `=`, non-ASCII bytes — is `%XX`-encoded).
/// Master names are free-form text and can contain all of those.
fn percent_encode_query_value(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Strip a trailing `Name [N]` counter (the unique-name engine's normal
/// form).
fn strip_name_counter(name: &str) -> String {
    if let Some(pos) = name.rfind(" [") {
        let tail = &name[pos + 2..];
        if tail.len() > 1 && tail.ends_with(']') && tail[..tail.len() - 1].chars().all(|c| c.is_ascii_digit()) {
            return name[..pos].trim_end().to_string();
        }
    }
    name.to_string()
}

/// Mint one random [A-Za-z0-9] code of the given length.
fn random_code(len: usize) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// Build the redirect target: the stored URL with `utm_campaign`,
/// `utm_source`, `utm_medium` query params set from the tracker's masters
/// (upstream's redirected_url compute). Existing utm_* params on the target
/// are REPLACED; everything else is preserved verbatim.
fn inject_utm_params(
    url: &str,
    campaign: Option<&str>,
    source: Option<&str>,
    medium: Option<&str>,
) -> String {
    let (before_fragment, fragment) = match url.split_once('#') {
        Some((base, frag)) => (base, Some(frag)),
        None => (url, None),
    };
    let (base, existing_query) = match before_fragment.split_once('?') {
        Some((b, q)) => (b, Some(q)),
        None => (before_fragment, None),
    };

    let wanted: [(&str, Option<&str>); 3] = [
        ("utm_campaign", campaign),
        ("utm_source", source),
        ("utm_medium", medium),
    ];

    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(query) = existing_query {
        for pair in query.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (key, value) = match pair.split_once('=') {
                Some((k, v)) => (k, v),
                None => (pair, ""),
            };
            if wanted.iter().any(|(name, _)| key.eq_ignore_ascii_case(name)) {
                continue; // replaced below
            }
            params.push((key.to_string(), value.to_string()));
        }
    }
    for (name, value) in wanted {
        if let Some(v) = value {
            params.push((name.to_string(), percent_encode_query_value(v)));
        }
    }

    let mut out = String::from(base);
    if !params.is_empty() {
        out.push('?');
        out.push_str(
            &params
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("&"),
        );
    }
    if let Some(frag) = fragment {
        out.push('#');
        out.push_str(frag);
    }
    out
}

// ─── the service ──────────────────────────────────────────────────────────────

pub struct EngagementWriteService {
    pool: PgPool,
}

impl EngagementWriteService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Resolve raw attribution strings to master ids, minting what is
    /// missing (the validated port of utm's find-or-create ferry).
    ///
    /// Minted campaigns carry `is_auto_campaign = true` so lists can filter
    /// cookie-minted rows from authored ones. The name grain is
    /// case-insensitive equality (upstream's `=ilike`).
    pub async fn resolve_attribution(
        &self,
        input: &AttributionInput,
        actor: Option<Uuid>,
    ) -> Result<AttributionRefs, EngagementError> {
        let campaign = match &input.campaign {
            Some(raw) => Some(clean_attribution_value(raw)?),
            None => None,
        };
        let source = match &input.source {
            Some(raw) => Some(clean_attribution_value(raw)?),
            None => None,
        };
        let medium = match &input.medium {
            Some(raw) => Some(clean_attribution_value(raw)?),
            None => None,
        };

        let mut tx = self.pool.begin().await?;

        let campaign_id = match campaign {
            Some(name) => {
                if let Some(id) =
                    EngagementRepository::find_campaign_by_name(&mut tx, &name).await?
                {
                    Some(id)
                } else {
                    let id = Uuid::new_v4();
                    let identifier = Self::free_campaign_identifier(&mut tx, &name).await?;
                    EngagementRepository::insert_campaign(
                        &mut tx, id, &identifier, &name, true, actor, actor,
                    )
                    .await?;
                    Some(id)
                }
            }
            None => None,
        };

        let source_id = match source {
            Some(name) => {
                if let Some(id) = EngagementRepository::find_source_by_name(&mut tx, &name).await? {
                    Some(id)
                } else {
                    let id = Uuid::new_v4();
                    EngagementRepository::insert_source(&mut tx, id, &name, actor).await?;
                    Some(id)
                }
            }
            None => None,
        };

        let medium_id = match medium {
            Some(name) => {
                if let Some(id) = EngagementRepository::find_medium_by_name(&mut tx, &name).await? {
                    Some(id)
                } else {
                    let id = Uuid::new_v4();
                    EngagementRepository::insert_medium(&mut tx, id, &name, actor).await?;
                    Some(id)
                }
            }
            None => None,
        };

        tx.commit().await?;
        Ok(AttributionRefs {
            campaign_id,
            source_id,
            medium_id,
        })
    }

    /// Author a campaign (the operator path, as opposed to the minted path
    /// of [`Self::resolve_attribution`]): a human title plus an optional
    /// explicit identifier. Without one, the identifier mints from the title
    /// through the unique-name engine; with one, a collision is a typed
    /// refusal. Authored campaigns carry `is_auto_campaign = false`.
    pub async fn create_campaign(
        &self,
        title: &str,
        explicit_name: Option<&str>,
        user_id: Option<Uuid>,
    ) -> Result<AttributionRefs, EngagementError> {
        let title = clean_attribution_value(title)?;
        let name = match explicit_name {
            Some(raw) => clean_attribution_value(raw)?,
            None => String::new(),
        };

        let mut tx = self.pool.begin().await?;
        let identifier = match explicit_name {
            Some(_) => {
                if EngagementRepository::campaign_name_taken(&mut tx, &name).await? {
                    return Err(EngagementError::CampaignNameTaken);
                }
                name
            }
            None => Self::free_campaign_identifier(&mut tx, &title).await?,
        };
        let id = Uuid::new_v4();
        EngagementRepository::insert_campaign(&mut tx, id, &identifier, &title, false, user_id, user_id)
            .await?;
        tx.commit().await?;
        Ok(AttributionRefs {
            campaign_id: Some(id),
            source_id: None,
            medium_id: None,
        })
    }

    /// The first free campaign identifier for a title: the title's normal
    /// form, then `Title [N]` counters from 1 (upstream's hole-filling
    /// engine, simplified to first-free rather than hole-scanning).
    async fn free_campaign_identifier(
        conn: &mut sqlx::PgConnection,
        title: &str,
    ) -> Result<String, EngagementError> {
        let base = strip_name_counter(title);
        if !EngagementRepository::campaign_name_taken(conn, &base).await? {
            return Ok(base);
        }
        for n in 1..=1000u32 {
            let candidate = format!("{base} [{n}]");
            if !EngagementRepository::campaign_name_taken(conn, &candidate).await? {
                return Ok(candidate);
            }
        }
        Err(EngagementError::CampaignNameExhausted)
    }

    /// Mint a short code not held by any live tracker: random [A-Za-z0-9]
    /// starting at [`MIN_CODE_LENGTH`]; ANY collision grows the length and
    /// retries (upstream's retry loop, with a ceiling).
    async fn mint_code(&self) -> Result<String, EngagementError> {
        let mut tx = self.pool.begin().await?;
        let mut len = MIN_CODE_LENGTH;
        while len <= MAX_CODE_LENGTH {
            let candidate = random_code(len);
            if !EngagementRepository::code_taken(&mut tx, &candidate).await? {
                tx.commit().await?;
                return Ok(candidate);
            }
            len += 1;
        }
        Err(EngagementError::CodeSpaceExhausted)
    }

    /// Create (or find) a tracker for the 5-tuple (url, campaign, medium,
    /// source, label) — the idempotent entry rendering pipelines call.
    ///
    /// The target must be absolute http(s); the label normalizes to
    /// empty-string when absent (part of the uniqueness key); the title, if
    /// any, is CALLER-SUPPLIED — no server-side fetch is ever performed.
    pub async fn create_tracker(
        &self,
        url: &str,
        title: Option<&str>,
        label: Option<&str>,
        attribution: &AttributionRefs,
        actor: Option<Uuid>,
    ) -> Result<TrackerView, EngagementError> {
        let url = validate_absolute_http_url(url)?;
        let title = match title {
            Some(t) => {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    if trimmed.chars().count() > MAX_TITLE_LEN {
                        return Err(EngagementError::TitleTooLong);
                    }
                    if trimmed.chars().any(|c| c.is_control()) {
                        return Err(EngagementError::TitleTooLong);
                    }
                    Some(trimmed.to_string())
                }
            }
            None => None,
        };
        let label = match label {
            Some(l) => {
                let trimmed = l.trim();
                if trimmed.chars().count() > MAX_LABEL_LEN {
                    return Err(EngagementError::LabelTooLong);
                }
                if trimmed.chars().any(|c| c.is_control()) {
                    return Err(EngagementError::LabelTooLong);
                }
                trimmed.to_string()
            }
            None => String::new(),
        };

        // Idempotent arm: an existing live tracker for the same tuple is
        // returned as-is (search-or-create).
        {
            let mut tx = self.pool.begin().await?;
            if let Some(existing) = EngagementRepository::find_tracker_by_tuple(
                &mut tx,
                &url,
                attribution.campaign_id,
                attribution.medium_id,
                attribution.source_id,
                &label,
            )
            .await?
            {
                let count = EngagementRepository::click_count(&mut tx, existing.id).await?;
                tx.commit().await?;
                return Ok(TrackerView {
                    id: existing.id,
                    url: existing.url,
                    code: existing.code,
                    title: existing.title,
                    label: existing.label,
                    campaign_id: existing.campaign_id,
                    medium_id: existing.medium_id,
                    source_id: existing.source_id,
                    click_count: count,
                    created: false,
                });
            }
            tx.commit().await?;
        }

        let code = self.mint_code().await?;
        let mut tx = self.pool.begin().await?;
        // Re-probe after the code mint: a concurrent create for the same
        // tuple may have won (upstream's constraint is check-then-act by
        // design — EN-42; we simply prefer the winner instead of minting a
        // duplicate).
        if let Some(existing) = EngagementRepository::find_tracker_by_tuple(
            &mut tx,
            &url,
            attribution.campaign_id,
            attribution.medium_id,
            attribution.source_id,
            &label,
        )
        .await?
        {
            let count = EngagementRepository::click_count(&mut tx, existing.id).await?;
            tx.commit().await?;
            return Ok(TrackerView {
                id: existing.id,
                url: existing.url,
                code: existing.code,
                title: existing.title,
                label: existing.label,
                campaign_id: existing.campaign_id,
                medium_id: existing.medium_id,
                source_id: existing.source_id,
                click_count: count,
                created: false,
            });
        }

        let tracker = EngagementRepository::insert_tracker(
            &mut tx,
            Uuid::new_v4(),
            &url,
            &code,
            title.as_deref(),
            &label,
            attribution.campaign_id,
            attribution.medium_id,
            attribution.source_id,
            actor,
        )
        .await?;
        tx.commit().await?;

        Ok(TrackerView {
            id: tracker.id,
            url: tracker.url,
            code: tracker.code,
            title: tracker.title,
            label: tracker.label,
            campaign_id: tracker.campaign_id,
            medium_id: tracker.medium_id,
            source_id: tracker.source_id,
            click_count: 0,
            created: true,
        })
    }

    /// Resolve a short code to its redirect target, minting a deduped click.
    ///
    /// Idempotency: one counted click per (tracker, ip, UTC day) — the
    /// ADR-0019 action_link contract (repeated activation converges). A cap
    /// bounds rows per tracker; hitting it stops counting, never redirecting.
    /// The stored target is RE-VALIDATED before use (defense in depth against
    /// a raw-written row).
    pub async fn resolve_redirect(
        &self,
        code: &str,
        ip: Option<&str>,
        country_code: Option<&str>,
    ) -> Result<RedirectTarget, EngagementError> {
        let mut tx = self.pool.begin().await?;

        let tracker = EngagementRepository::find_tracker_by_code(&mut tx, code)
            .await?
            .ok_or(EngagementError::TrackerNotFound)?;

        // No open redirect, ever: only a stored target that still passes the
        // absolute-http(s) gate is honored.
        let url = validate_absolute_http_url(&tracker.url)
            .map_err(|_| EngagementError::StoredTargetInvalid)?;

        let (campaign_name, medium_name, source_name) = EngagementRepository::master_names(
            &mut tx,
            tracker.campaign_id,
            tracker.medium_id,
            tracker.source_id,
        )
        .await?;

        // Mint the click (dedup + cap). Replays converge; cap overflow
        // freezes counting but never the redirect.
        let clicks = EngagementRepository::click_count(&mut tx, tracker.id).await?;
        if clicks < MAX_CLICKS_PER_TRACKER {
            let day = Utc::now().date_naive();
            let ip_key = ip.unwrap_or("unknown");
            let dedup_key = format!("{}:{}:{}", tracker.id, ip_key, day);
            let ip_clean = ip.and_then(|v| {
                let trimmed = v.trim();
                if trimmed.is_empty() || trimmed.chars().count() > 64 {
                    None
                } else {
                    Some(trimmed.to_string())
                }
            });
            let cc_clean = country_code.and_then(|v| {
                let trimmed = v.trim();
                if trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
                    Some(trimmed.to_ascii_uppercase())
                } else {
                    None
                }
            });
            let inserted = EngagementRepository::insert_click(
                &mut tx,
                Uuid::new_v4(),
                tracker.id,
                tracker.campaign_id,
                ip_clean.as_deref(),
                cc_clean.as_deref(),
                day,
                &dedup_key,
            )
            .await?;
            let _ = inserted;
        } else {
            tracing::warn!(
                tracker = %tracker.id,
                cap = MAX_CLICKS_PER_TRACKER,
                "tracker click cap reached — counting frozen, redirect proceeds"
            );
        }

        tx.commit().await?;

        Ok(RedirectTarget {
            url: inject_utm_params(
                &url,
                campaign_name.as_deref(),
                source_name.as_deref(),
                medium_name.as_deref(),
            ),
        })
    }

    /// A tracker's detail (row + live click count).
    pub async fn tracker_detail(&self, tracker_id: Uuid) -> Result<TrackerView, EngagementError> {
        let mut conn = self.pool.acquire().await?;
        let tracker = EngagementRepository::find_tracker_by_id(&mut conn, tracker_id)
            .await?
            .ok_or(EngagementError::TrackerNotFound)?;
        let count = EngagementRepository::click_count(&mut conn, tracker.id).await?;
        Ok(TrackerView {
            id: tracker.id,
            url: tracker.url,
            code: tracker.code,
            title: tracker.title,
            label: tracker.label,
            campaign_id: tracker.campaign_id,
            medium_id: tracker.medium_id,
            source_id: tracker.source_id,
            click_count: count,
            created: false,
        })
    }

    /// Total deduped clicks recorded against a campaign (reads the clicks'
    /// own denormalized campaign — attribution edits on the tracker do not
    /// rewrite history).
    pub async fn campaign_click_total(&self, campaign_id: Uuid) -> Result<i64, EngagementError> {
        let mut conn = self.pool.acquire().await?;
        Ok(EngagementRepository::campaign_click_total(&mut conn, campaign_id).await?)
    }
}
