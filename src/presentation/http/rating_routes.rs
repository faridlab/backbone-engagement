//! Rating routes (hand-written; user-owned).
//!
//! Two mounts, one principle: **the capability link is the auth**.
//!
//! - **Public** — `POST /rate/:token/submit`, a BARE (unauthenticated)
//!   mount throttled 120/min per client, the same ADR-0019 action_link
//!   class as the short-link redirect: the token in the path is an
//!   unguessable HMAC'd capability, the rater is anonymous by design, and
//!   every refusal after MAC verification answers the shared
//!   `not_submittable` shape — already-consumed, unknown, expired, and
//!   forged are indistinguishable to the caller (no oracle). Mount at the
//!   site root.
//! - **Guarded** — issue / rotate / reset / publisher-reply / stats
//!   composers over [`RatingWriteService`], for the host's authenticated
//!   operator tree. No generic rating CRUD is mounted anywhere: rating
//!   rows are ONLY written through the validated verbs (the read-only
//!   generated router stays available via the module's readonly base).
//!
//! Route map (relative to each mount point):
//!
//! | Mount | Method | Path | Handler |
//! |---|---|---|---|
//! | public | POST | /rate/:token/submit | anonymous single-use submit |
//! | guarded | POST | /engagement/ratings | issue (or reuse) a request |
//! | guarded | POST | /engagement/ratings/:id/rotate | fresh nonce + expiry |
//! | guarded | POST | /engagement/ratings/:id/reset | clear answer, re-arm |
//! | guarded | POST | /engagement/ratings/:id/reply | publisher reply |
//! | guarded | GET | /engagement/ratings/stats | one record's aggregate |
//! | guarded | GET | /engagement/ratings/stats/parent | parent rollup |

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::application::service::rating_write_service::RatingError;
use crate::EngagementModule;

use super::redirect_routes::ApiState;

fn rating_err(e: RatingError) -> Response {
    let status =
        StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = match &e {
        RatingError::Db(_) | RatingError::Internal(_) | RatingError::SecretNotConfigured => {
            json!({ "error": "internal error", "code": "internal_error" })
        }
        other => json!({ "error": other.to_string(), "code": other.code() }),
    };
    (status, Json(body)).into_response()
}

// ─── the public submit ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SubmitRatingBody {
    /// 1..=10 — validated BEFORE any token work (a bad value answers 422
    /// without touching the capability).
    pub rating_value: i32,
    pub feedback: Option<String>,
}

async fn submit_rating(
    State(app): State<ApiState>,
    Path(token): Path<String>,
    Json(body): Json<SubmitRatingBody>,
) -> Response {
    let service = app.rating_write_service();
    match service
        .submit_rating(&token, body.rating_value, body.feedback.as_deref())
        .await
    {
        Ok(rating_id) => (StatusCode::OK, Json(json!({ "ratingId": rating_id }))).into_response(),
        // The shared refusal: consumed / unknown / expired / forged or
        // malformed all land here, one shape, one status — the capability
        // carries no oracle. Validation-shaped failures (value bounds,
        // feedback length) keep their own 422s; they reveal nothing about
        // the token.
        Err(RatingError::NotSubmittable | RatingError::TokenMalformed) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "not submittable", "code": "not_submittable" })),
        )
            .into_response(),
        Err(e) => rating_err(e),
    }
}

/// The PUBLIC rating group — BARE mount (the token is the capability),
/// throttled 120/min per client (ADR-0019: the submit is a mutating
/// action reached from an emailed link; rate limiting applies regardless
/// of token strength).
pub fn public_composer() -> Router<ApiState> {
    use axum::middleware as axum_mw;

    Router::<ApiState>::new()
        .route("/rate/:token/submit", post(submit_rating))
        .route_layer(axum_mw::from_fn_with_state(
            backbone_rate_limit::middleware(120, 60),
            backbone_rate_limit::rate_limit_middleware,
        ))
}

// ─── the guarded management seams ─────────────────────────────────────────────

#[derive(Deserialize)]
pub struct IssueRatingBody {
    pub rated_model: String,
    pub rated_record_id: Uuid,
    pub rater_user_id: Option<Uuid>,
    pub rater_email: Option<String>,
    pub rated_user_id: Option<Uuid>,
    pub parent_rated_record_id: Option<Uuid>,
    pub ttl_days: Option<i64>,
}

async fn issue_rating(State(app): State<ApiState>, Json(body): Json<IssueRatingBody>) -> Response {
    match app
        .rating_write_service()
        .issue_rating(
            &body.rated_model,
            body.rated_record_id,
            body.rater_user_id,
            body.rater_email.as_deref(),
            body.rated_user_id,
            body.parent_rated_record_id,
            body.ttl_days,
        )
        .await
    {
        Ok(view) => (StatusCode::CREATED, Json(view)).into_response(),
        Err(e) => rating_err(e),
    }
}

async fn rotate_token(
    State(app): State<ApiState>,
    Path(id): Path<Uuid>,
    body: Option<Json<TtlBody>>,
) -> Response {
    let ttl = body.and_then(|Json(b)| b.ttl_days);
    match app.rating_write_service().rotate_token(id, ttl).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(e) => rating_err(e),
    }
}

async fn reset_rating(
    State(app): State<ApiState>,
    Path(id): Path<Uuid>,
    body: Option<Json<TtlBody>>,
) -> Response {
    let ttl = body.and_then(|Json(b)| b.ttl_days);
    match app.rating_write_service().reset_rating(id, ttl).await {
        Ok(view) => (StatusCode::OK, Json(view)).into_response(),
        Err(e) => rating_err(e),
    }
}

#[derive(Deserialize)]
pub struct TtlBody {
    pub ttl_days: Option<i64>,
}

#[derive(Deserialize)]
pub struct PublisherReplyBody {
    pub comment: String,
    pub publisher_user_id: Uuid,
}

async fn publisher_reply(
    State(app): State<ApiState>,
    Path(id): Path<Uuid>,
    Json(body): Json<PublisherReplyBody>,
) -> Response {
    match app
        .rating_write_service()
        .publisher_reply(id, &body.comment, body.publisher_user_id)
        .await
    {
        Ok(()) => (StatusCode::NO_CONTENT).into_response(),
        Err(e) => rating_err(e),
    }
}

#[derive(Deserialize)]
pub struct RecordStatsQuery {
    pub rated_model: String,
    pub rated_record_id: Uuid,
}

async fn record_stats(State(app): State<ApiState>, Query(q): Query<RecordStatsQuery>) -> Response {
    match app
        .rating_write_service()
        .record_stats(&q.rated_model, q.rated_record_id)
        .await
    {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => rating_err(e),
    }
}

#[derive(Deserialize)]
pub struct ParentStatsQuery {
    pub parent_rated_model: String,
    pub parent_rated_record_id: Uuid,
}

async fn parent_stats(State(app): State<ApiState>, Query(q): Query<ParentStatsQuery>) -> Response {
    match app
        .rating_write_service()
        .parent_stats(&q.parent_rated_model, q.parent_rated_record_id)
        .await
    {
        Ok(stats) => (StatusCode::OK, Json(stats)).into_response(),
        Err(e) => rating_err(e),
    }
}

/// The guarded rating router: the validated operator seams only. Mount
/// behind the host's authenticated tree.
pub fn guarded_composer(m: std::sync::Arc<EngagementModule>) -> Router {
    Router::new()
        .route("/engagement/ratings", post(issue_rating))
        .route("/engagement/ratings/:id/rotate", post(rotate_token))
        .route("/engagement/ratings/:id/reset", post(reset_rating))
        .route("/engagement/ratings/:id/reply", post(publisher_reply))
        .route("/engagement/ratings/stats", get(record_stats))
        .route("/engagement/ratings/stats/parent", get(parent_stats))
        .with_state(m)
}
