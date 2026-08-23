//! Link-tracker write-path cases (hand-written; user-owned).
//!
//! Exercises [`EngagementWriteService`]'s tracker minting (idempotent on the
//! 5-tuple, absolute-http(s) target gate, length gates), the redirect
//! resolver (utm injection with percent-encoding, the per-(tracker, ip, day)
//! click dedup, unknown codes), and the HTTP surfaces as they mount: the
//! guarded management composition and the public throttled `/r/:code`
//! redirect, probed in-process via `tower`'s `oneshot`. Each test runs on
//! its own scratch database (`sqlx::test`) with the module migrations
//! applied.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use backbone_engagement::EngagementModule;
use backbone_engagement::application::service::engagement_write_service::{
    AttributionInput, AttributionRefs, EngagementError, EngagementWriteService, MIN_CODE_LENGTH,
};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

/// Resolve a fresh attribution triple for test data isolation.
async fn fresh_attribution(svc: &EngagementWriteService, seed: &str) -> AttributionRefs {
    svc.resolve_attribution(
        &AttributionInput {
            campaign: Some(format!("{seed} Campaign")),
            source: Some(format!("{seed} Source")),
            medium: Some(format!("{seed} Medium")),
        },
        None,
    )
    .await
    .expect("attribution")
}

fn is_alnum_code(code: &str) -> bool {
    code.len() >= MIN_CODE_LENGTH
        && code.chars().all(|c| c.is_ascii_alphanumeric())
}

// ─── minting ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn minting_a_tracker_yields_a_short_alnum_code(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = fresh_attribution(&svc, "Mint").await;

    let view = svc
        .create_tracker("https://example.com/promo", Some("Promo page"), Some("footer"), &refs, None)
        .await
        .expect("mint");

    assert!(view.created, "first call mints");
    assert!(is_alnum_code(&view.code), "code must be [A-Za-z0-9], got {:?}", view.code);
    assert_eq!(view.url, "https://example.com/promo");
    assert_eq!(view.title.as_deref(), Some("Promo page"));
    assert_eq!(view.label, "footer");
    assert_eq!(view.campaign_id, refs.campaign_id);
    assert_eq!(view.click_count, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn the_five_tuple_is_idempotent_and_label_is_part_of_it(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = fresh_attribution(&svc, "Tuple").await;

    let url = "https://example.com/page?x=1";
    let first = svc.create_tracker(url, None, Some("a"), &refs, None).await.expect("first");
    let again = svc.create_tracker(url, None, Some("a"), &refs, None).await.expect("again");

    assert!(first.created);
    assert!(!again.created, "same 5-tuple must return the existing tracker");
    assert_eq!(first.id, again.id);
    assert_eq!(again.code, first.code);

    // A different label is a different tuple → a second tracker.
    let other = svc.create_tracker(url, None, Some("b"), &refs, None).await.expect("other label");
    assert!(other.created);
    assert_ne!(other.id, first.id);

    // Absent label normalizes to empty string — also part of the key.
    let no_label = svc.create_tracker(url, None, None, &refs, None).await.expect("no label");
    assert!(no_label.created);
    assert_eq!(no_label.label, "");
    assert_ne!(no_label.id, first.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn attribution_nones_and_refs_both_mint(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = AttributionRefs { campaign_id: None, source_id: None, medium_id: None };
    let view = svc
        .create_tracker("https://example.com/bare", None, None, &refs, None)
        .await
        .expect("bare tracker");
    assert!(view.created);
    assert_eq!(view.campaign_id, None);
}

// ─── the target gate ─────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn non_absolute_http_targets_are_refused(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = AttributionRefs::default();

    for bad in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "data:text/html;base64,eHg=",
        "/relative/path",
        "//example.com/scheme-less",
        "ftp://example.com/file",
        "example.com/no-scheme",
        "http://:",
        "https://exa mple.com",
    ] {
        let err = svc.create_tracker(bad, None, None, &refs, None).await.expect_err("must refuse");
        assert!(
            matches!(err, EngagementError::NotAbsoluteHttpUrl),
            "expected NotAbsoluteHttpUrl for {bad:?}, got {err:?}"
        );
    }

    // Nothing was written while refusing.
    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM engagement.engagement_link_trackers",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn length_gates_are_enforced(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = AttributionRefs::default();

    let err = svc
        .create_tracker(&format!("https://example.com/{}", "a".repeat(2001)), None, None, &refs, None)
        .await
        .expect_err("url too long");
    assert!(matches!(err, EngagementError::UrlTooLong));

    let err = svc
        .create_tracker("https://example.com/x", None, Some(&"l".repeat(41)), &refs, None)
        .await
        .expect_err("label too long");
    assert!(matches!(err, EngagementError::LabelTooLong));

    let err = svc
        .create_tracker("https://example.com/x", Some(&"t".repeat(301)), None, &refs, None)
        .await
        .expect_err("title too long");
    assert!(matches!(err, EngagementError::TitleTooLong));
}

// ─── the redirect resolver ───────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn redirect_injects_encoded_utms_and_replaces_stale_ones(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    // Master names with characters that MUST be percent-encoded as query
    // values (spaces), to prove the injection cannot corrupt the query.
    let refs = svc
        .resolve_attribution(
            &AttributionInput {
                campaign: Some("Summer Sale".into()),
                source: Some("News Letter".into()),
                medium: Some("Email".into()),
            },
            None,
        )
        .await
        .expect("attribution");

    let view = svc
        .create_tracker(
            "https://example.com/promo?a=1&utm_campaign=stale&utm_source=stale#section",
            None,
            None,
            &refs,
            None,
        )
        .await
        .expect("tracker");

    let target = svc
        .resolve_redirect(&view.code, Some("203.0.113.7"), Some("id"))
        .await
        .expect("redirect");

    assert!(
        target.url.starts_with("https://example.com/promo?a=1"),
        "existing non-utm params must be preserved: {}",
        target.url
    );
    assert!(target.url.contains("utm_campaign=Summer%20Sale"), "in: {}", target.url);
    assert!(target.url.contains("utm_source=News%20Letter"), "in: {}", target.url);
    assert!(target.url.contains("utm_medium=Email"), "in: {}", target.url);
    assert!(!target.url.contains("stale"), "stale utm_* values must be replaced");
    assert!(target.url.ends_with("#section"), "fragment must survive: {}", target.url);
}

#[sqlx::test(migrations = "./migrations")]
async fn unknown_codes_are_a_typed_not_found(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let err = svc.resolve_redirect("zz9notreal", None, None).await.expect_err("unknown code");
    assert!(matches!(err, EngagementError::TrackerNotFound));
}

#[sqlx::test(migrations = "./migrations")]
async fn clicks_dedup_per_tracker_ip_and_day(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = fresh_attribution(&svc, "Dedup").await;
    let view = svc
        .create_tracker("https://example.com/dedup", None, None, &refs, None)
        .await
        .expect("tracker");

    // Same ip, same UTC day: replays converge on ONE counted click.
    for _ in 0..3 {
        svc.resolve_redirect(&view.code, Some("203.0.113.7"), None).await.expect("replay");
    }
    // A different ip counts.
    svc.resolve_redirect(&view.code, Some("198.51.100.9"), None).await.expect("second ip");
    // An absent ip degrades to the shared "unknown" bucket — still deduped.
    for _ in 0..2 {
        svc.resolve_redirect(&view.code, None, None).await.expect("no-ip replay");
    }

    let detail = svc.tracker_detail(view.id).await.expect("detail");
    assert_eq!(detail.click_count, 3, "ip + ip + unknown = 3 counted clicks");

    // The campaign total reads the clicks' own denormalized campaign.
    let total = svc
        .campaign_click_total(refs.campaign_id.expect("campaign id"))
        .await
        .expect("campaign total");
    assert_eq!(total, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn campaign_totals_sum_across_the_campaigns_trackers(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = fresh_attribution(&svc, "Sum").await;
    let one = svc
        .create_tracker("https://example.com/one", None, Some("l1"), &refs, None)
        .await
        .expect("tracker one");
    let two = svc
        .create_tracker("https://example.com/two", None, Some("l2"), &refs, None)
        .await
        .expect("tracker two");

    svc.resolve_redirect(&one.code, Some("203.0.113.1"), None).await.expect("click one");
    svc.resolve_redirect(&two.code, Some("203.0.113.1"), None).await.expect("click two");
    svc.resolve_redirect(&two.code, Some("203.0.113.2"), None).await.expect("click two bis");

    let total = svc
        .campaign_click_total(refs.campaign_id.expect("campaign id"))
        .await
        .expect("campaign total");
    assert_eq!(total, 3);
    assert_eq!(svc.tracker_detail(two.id).await.expect("detail").click_count, 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn tracker_detail_reports_unknown_ids_as_not_found(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let err = svc.tracker_detail(Uuid::new_v4()).await.expect_err("unknown id");
    assert!(matches!(err, EngagementError::TrackerNotFound));
}

// ─── the mounted HTTP surfaces (in-process probes) ──────────────────────────

async fn module(pool: PgPool) -> Arc<EngagementModule> {
    Arc::new(
        EngagementModule::builder()
            .with_database(pool)
            .build()
            .expect("module builds"),
    )
}

async fn body_string(body: Body) -> String {
    let bytes = to_bytes(body, usize::MAX).await.expect("read body");
    String::from_utf8(bytes.to_vec()).expect("utf8 body")
}

#[sqlx::test(migrations = "./migrations")]
async fn public_redirect_route_answers_302_and_404(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = fresh_attribution(&svc, "Route").await;
    let view = svc
        .create_tracker("https://example.com/route-target", None, None, &refs, None)
        .await
        .expect("tracker");
    let app = module(pool).await.redirect_routes();

    // A known code: 302 with the injected target.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/r/{}", view.code))
                .header("x-forwarded-for", "203.0.113.7")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::FOUND, "known codes redirect (302, never a cached 301)");
    let location = res.headers().get(header::LOCATION).expect("location header").to_str().unwrap().to_string();
    assert!(location.starts_with("https://example.com/route-target"), "location: {location}");
    assert!(location.contains("utm_campaign="), "utm injected into: {location}");

    // An unknown code: a bare 404 with no detail (nothing to enumerate).
    let res = app
        .oneshot(
            Request::builder()
                .uri("/r/definitely-not-a-code")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body = body_string(res.into_body()).await;
    assert_eq!(body, "not found");
}

#[sqlx::test(migrations = "./migrations")]
async fn guarded_routes_serve_writes_and_reads(pool: PgPool) {
    let app = module(pool.clone()).await.guarded_routes();

    // The attribution seam.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/engagement/attribution/resolve")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "campaign": "Guarded Campaign", "source": "Web", "medium": "cpc" })
                        .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&body_string(res.into_body()).await).expect("json body");
    let campaign_id: Uuid = body["campaignId"].as_str().expect("campaignId").parse().expect("uuid");
    let source_id: Uuid = body["sourceId"].as_str().expect("sourceId").parse().expect("uuid");
    let medium_id: Uuid = body["mediumId"].as_str().expect("mediumId").parse().expect("uuid");

    // Tracker minting through the guarded write path.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/engagement/link_trackers")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "url": "https://example.com/guarded",
                        "title": "Guarded target",
                        "campaign_id": campaign_id,
                        "medium_id": medium_id,
                        "source_id": source_id,
                    })
                    .to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value =
        serde_json::from_str(&body_string(res.into_body()).await).expect("json body");
    let tracker_id: Uuid = body["id"].as_str().expect("id").parse().expect("uuid");
    assert_eq!(body["created"], serde_json::json!(true));

    // A javascript: target through the HTTP surface: refused with 422.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/engagement/link_trackers")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    serde_json::json!({ "url": "javascript:alert(1)" }).to_string(),
                ))
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);

    // Detail read: row + live click count.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/engagement/link_trackers/{tracker_id}/detail"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&body_string(res.into_body()).await).expect("json body");
    assert_eq!(body["clickCount"], serde_json::json!(0));

    // Campaign click total read.
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/engagement/campaigns/{campaign_id}/clicks"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_str(&body_string(res.into_body()).await).expect("json body");
    assert_eq!(body["clickCount"], serde_json::json!(0));

    // The generated GET-only management reads are mounted too.
    let res = app
        .oneshot(
            Request::builder()
                .uri("/engagement_campaigns")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("oneshot");
    assert_eq!(res.status(), StatusCode::OK);
}
