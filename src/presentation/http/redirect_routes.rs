//! The public short-link redirect (hand-written; user-owned).
//!
//! `GET /r/:code` — the port of link_tracker's public 301 redirector, as an
//! **ADR-0019 action_link-class** route: a link in an email or on a website
//! genuinely cannot be a POST, so it mounts as the sanctioned exception with
//! all three of the ADR's requirements satisfied —
//!
//! - **Idempotency**: repeated activation converges. The click mint rides a
//!   (tracker, ip, UTC-day) dedup key, so a replay mints nothing; only the
//!   first hit of a day counts.
//! - **Explicit declaration**: this file is the declaration surface — the
//!   route exists ONLY here, greppable, throttled, and it mutates nothing
//!   beyond the click row bound to the resolved code.
//! - **Throttling at middleware**: the lookup distinguishes exists / doesn't
//!   (redirect vs 404), so the group is rate-limited regardless.
//!
//! Hardening deltas over upstream (decided, not transcribed):
//! - **No open redirect** — the stored target must be absolute http(s) and
//!   is re-validated at resolve time; anything else fails closed.
//! - **302, not 301** — upstream's permanent redirect pins the target
//!   client-side; a tracker whose target was edited (or whose UTM masters
//!   changed) must propagate on the next hit, which a cached 301 forbids.
//! - **Dedup + cap** — see the write service.
//!
//! The group mounts BARE (no auth middleware — the short code is an
//! unguessable capability, and the visitor is anonymous by design). Mount it
//! at the site root so codes resolve as `/r/<code>`.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::application::service::engagement_write_service::EngagementError;
use crate::application::service::engagement_write_service::EngagementWriteService;
use crate::EngagementModule;

/// Shared route state (the module handle).
pub type ApiState = Arc<EngagementModule>;

/// The visitor IP as the host saw it: the first hop of `X-Forwarded-For`
/// when the host sits behind a proxy, else none (the dedup grain degrades to
/// "unknown", one counted click per tracker per day — never a mint storm).
fn visitor_ip(headers: &HeaderMap) -> Option<String> {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

async fn redirect(
    State(app): State<ApiState>,
    headers: HeaderMap,
    Path(code): Path<String>,
) -> Response {
    let service: &EngagementWriteService = app.engagement_write_service();
    let country = headers
        .get("x-visitor-country")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().to_string());
    match service
        .resolve_redirect(&code, visitor_ip(&headers).as_deref(), country.as_deref())
        .await
    {
        Ok(target) => (
            StatusCode::FOUND,
            [(
                axum::http::header::LOCATION,
                target.url,
            )],
        )
            .into_response(),
        // Every failure arm answers the same 404 with no detail — unknown
        // code, corrupt stored target, backend trouble: indistinguishable to
        // a public caller (the group throttle bounds the enumeration this
        // would otherwise enable; internals surface via the service's
        // tracing, not the response).
        Err(EngagementError::TrackerNotFound) | Err(_) => {
            (StatusCode::NOT_FOUND, "not found").into_response()
        }
    }
}

/// The public redirect route group — BARE mount (the short code is the
/// capability), throttled 120/min per client (ADR-0019: the exists/doesn't
/// distinction is an enumeration shape; rate limiting applies regardless of
/// token strength).
pub fn composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/r/:code", get(redirect))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(120, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}
