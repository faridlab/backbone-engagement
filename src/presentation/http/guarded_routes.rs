//! Guarded route composition — the RECOMMENDED way to mount the engagement
//! module's management surface (hand-written; user-owned).
//!
//! Deliberately does NOT mount [`crate::EngagementModule::all_crud_routes`]:
//! generic mutation would bypass the write service's invariants (mint
//! trackers with non-http targets or duplicate codes, mint masters without
//! the validation gate, rewrite click rows). Instead:
//!
//! - **Reads**: the generated GET-only routers for campaigns, sources,
//!   media, and trackers. The generic click router is NOT mounted — click
//!   rows carry visitor IPs; counting is exposed as aggregated reads
//!   (tracker detail, campaign click total) instead of raw-row listing.
//! - **Writes**: the attribution find-or-create seam and tracker minting
//!   flow through [`EngagementWriteService`] — validated, idempotent.
//!
//! This module is UNFENCED (ADR-0014 posture 4 — no company dimension, same
//! as upstream utm/link_tracker), so there is no company binding to carry:
//! mount behind the host's authentication tree and let the host apply its
//! own operator gates.
//!
//! The PUBLIC redirect (`/r/:code`) is a separate, bare mount — see
//! [`crate::presentation::http::redirect_routes`].
//!
//! Route map (relative to the mount point):
//!
//! | Method | Path | Handler |
//! |---|---|---|
//! | GET | /engagement/campaigns… | generated reads |
//! | GET | /engagement/sources… | generated reads |
//! | GET | /engagement/media… | generated reads |
//! | GET | /engagement/link_trackers… | generated reads |
//! | POST | /engagement/attribution/resolve | find-or-create the attribution triple |
//! | POST | /engagement/link_trackers | mint (or find) a tracker |
//! | GET | /engagement/link_trackers/:id/detail | row + live click count |
//! | GET | /engagement/campaigns/:id/clicks | campaign click total |

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::application::service::engagement_write_service::{
    AttributionInput, AttributionRefs, EngagementError, TrackerView,
};
use crate::EngagementModule;

use super::redirect_routes::ApiState;

fn engagement_err(e: EngagementError) -> Response {
    let status =
        StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    // Validation failures carry their reason (the caller is an authenticated
    // operator); internals stay generic.
    let body = match &e {
        EngagementError::Db(_) | EngagementError::Internal(_) => {
            json!({ "error": "internal error", "code": "internal_error" })
        }
        other => json!({ "error": other.to_string(), "code": other.code() }),
    };
    (status, Json(body)).into_response()
}

// ─── handlers ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ResolveAttributionBody {
    pub campaign: Option<String>,
    pub source: Option<String>,
    pub medium: Option<String>,
}

async fn resolve_attribution(
    State(app): State<ApiState>,
    Json(body): Json<ResolveAttributionBody>,
) -> Response {
    let input = AttributionInput {
        campaign: body.campaign,
        source: body.source,
        medium: body.medium,
    };
    match app
        .engagement_write_service()
        .resolve_attribution(&input, None)
        .await
    {
        Ok(refs) => (StatusCode::OK, Json(refs)).into_response(),
        Err(e) => engagement_err(e),
    }
}

#[derive(Deserialize)]
pub struct CreateTrackerBody {
    pub url: String,
    pub title: Option<String>,
    pub label: Option<String>,
    #[serde(default)]
    pub campaign_id: Option<Uuid>,
    #[serde(default)]
    pub medium_id: Option<Uuid>,
    #[serde(default)]
    pub source_id: Option<Uuid>,
}

async fn create_tracker(State(app): State<ApiState>, Json(body): Json<CreateTrackerBody>) -> Response {
    let attribution = AttributionRefs {
        campaign_id: body.campaign_id,
        medium_id: body.medium_id,
        source_id: body.source_id,
    };
    match app
        .engagement_write_service()
        .create_tracker(
            &body.url,
            body.title.as_deref(),
            body.label.as_deref(),
            &attribution,
            None,
        )
        .await
    {
        Ok(view) => (StatusCode::CREATED, Json(view)).into_response(),
        Err(e) => engagement_err(e),
    }
}

async fn tracker_detail(State(app): State<ApiState>, Path(id): Path<Uuid>) -> Response {
    match app.engagement_write_service().tracker_detail(id).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(e) => engagement_err(e),
    }
}

async fn campaign_clicks(State(app): State<ApiState>, Path(id): Path<Uuid>) -> Response {
    match app.engagement_write_service().campaign_click_total(id).await {
        Ok(total) => (
            StatusCode::OK,
            Json(json!({ "campaignId": id, "clickCount": total })),
        )
            .into_response(),
        Err(e) => engagement_err(e),
    }
}

// ─── composition ──────────────────────────────────────────────────────────────

/// Build the guarded engagement router: safe GETs + the validated write
/// seams, NO generic mutation. Mount behind the host's authenticated tree.
pub fn composer(m: Arc<EngagementModule>) -> Router {
    use crate::presentation::http::{
        create_engagement_campaign_read_routes, create_engagement_link_tracker_read_routes,
        create_engagement_medium_read_routes, create_engagement_source_read_routes,
    };

    let writes: Router = Router::new()
        .route("/engagement/attribution/resolve", post(resolve_attribution))
        .route("/engagement/link_trackers", post(create_tracker))
        .route("/engagement/link_trackers/:id/detail", get(tracker_detail))
        .route("/engagement/campaigns/:id/clicks", get(campaign_clicks))
        .with_state(m.clone());

    // The generated GET-only management reads (click rows deliberately not
    // listed — see the module doc above).
    let reads = Router::new()
        .merge(create_engagement_campaign_read_routes(
            m.engagement_campaign_service.clone(),
        ))
        .merge(create_engagement_source_read_routes(
            m.engagement_source_service.clone(),
        ))
        .merge(create_engagement_medium_read_routes(
            m.engagement_medium_service.clone(),
        ))
        .merge(create_engagement_link_tracker_read_routes(
            m.engagement_link_tracker_service.clone(),
        ));

    reads.merge(writes)
}
