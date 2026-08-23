//! Attribution write-path cases (hand-written; user-owned).
//!
//! Exercises [`EngagementWriteService`]'s master-data behavior against a
//! real Postgres: the find-or-create seam's case-insensitive name grain, the
//! validation gate in front of it, the unique-name engine, and the authored
//! vs minted campaign distinction. Each test runs on its own scratch
//! database (`sqlx::test`) with the module migrations applied.

use backbone_engagement::application::service::engagement_write_service::{
    AttributionInput, EngagementError, EngagementWriteService,
};
use sqlx::PgPool;
use uuid::Uuid;

/// Count live rows in a master table.
async fn live_count(pool: &PgPool, table: &str) -> i64 {
    sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM engagement.{table} WHERE (metadata->>'deleted_at') IS NULL"
    ))
    .fetch_one(pool)
    .await
    .expect("count rows")
}

/// Fetch a campaign's (name, is_auto_campaign) by id.
async fn campaign_row(pool: &PgPool, id: Uuid) -> (String, bool) {
    sqlx::query_as(
        "SELECT name, is_auto_campaign FROM engagement.engagement_campaigns WHERE id = $1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .expect("fetch campaign")
}

fn input(campaign: Option<&str>, source: Option<&str>, medium: Option<&str>) -> AttributionInput {
    AttributionInput {
        campaign: campaign.map(String::from),
        source: source.map(String::from),
        medium: medium.map(String::from),
    }
}

// ─── find-or-create ───────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn find_or_create_is_case_insensitive_and_idempotent(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let first = svc
        .resolve_attribution(&input(Some("Summer Sale"), Some("Newsletter"), Some("Email")), None)
        .await
        .expect("first resolve");
    // Same values, different casing: the name grain is case-insensitive
    // equality, so nothing new is minted.
    let second = svc
        .resolve_attribution(&input(Some("summer sale"), Some("NEWSLETTER"), Some("eMaIl")), None)
        .await
        .expect("second resolve");

    assert_eq!(first.campaign_id, second.campaign_id);
    assert_eq!(first.source_id, second.source_id);
    assert_eq!(first.medium_id, second.medium_id);

    assert_eq!(live_count(&pool, "engagement_campaigns").await, 1);
    assert_eq!(live_count(&pool, "engagement_sources").await, 1);
    assert_eq!(live_count(&pool, "engagement_media").await, 1);

    // All three ids must be present (attribution triple fully resolved).
    let (c, s, m) = (first.campaign_id, first.source_id, first.medium_id);
    assert!((c.is_some() && s.is_some() && m.is_some()));
}

#[sqlx::test(migrations = "./migrations")]
async fn distinct_names_mint_distinct_masters(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let a = svc
        .resolve_attribution(&input(Some("Summer Sale"), Some("Newsletter"), None), None)
        .await
        .expect("resolve a");
    let b = svc
        .resolve_attribution(&input(Some("Autumn Sale"), Some("Ads"), None), None)
        .await
        .expect("resolve b");

    assert_ne!(a.campaign_id, b.campaign_id);
    assert_ne!(a.source_id, b.source_id);
    assert_eq!(a.medium_id, None);
    assert_eq!(b.medium_id, None);
    assert_eq!(live_count(&pool, "engagement_campaigns").await, 2);
    assert_eq!(live_count(&pool, "engagement_sources").await, 2);
    assert_eq!(live_count(&pool, "engagement_media").await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn absent_values_resolve_to_nothing_and_mint_nothing(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let refs = svc.resolve_attribution(&input(None, None, None), None).await.expect("empty resolve");

    assert!(refs.campaign_id.is_none());
    assert!(refs.source_id.is_none());
    assert!(refs.medium_id.is_none());
    assert_eq!(live_count(&pool, "engagement_campaigns").await, 0);
    assert_eq!(live_count(&pool, "engagement_sources").await, 0);
    assert_eq!(live_count(&pool, "engagement_media").await, 0);
}

// ─── the validation gate ─────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn invalid_attribution_values_are_refused_before_anything_mints(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let too_long = "x".repeat(121);
    let cases: [(&str, fn(&EngagementError) -> bool); 3] = [
        ("   ", |e| matches!(e, EngagementError::EmptyAttributionValue)),
        (too_long.as_str(), |e| matches!(e, EngagementError::AttributionValueTooLong)),
        ("bad\u{0007}value", |e| matches!(e, EngagementError::AttributionValueControlChars)),
    ];

    for (raw, is_expected) in cases {
        // The gate applies to each of the three positions independently.
        for value in [
            input(Some(raw), None, None),
            input(None, Some(raw), None),
            input(None, None, Some(raw)),
        ] {
            let err = svc.resolve_attribution(&value, None).await.expect_err("must refuse");
            assert!(is_expected(&err), "unexpected error for {raw:?}: {err:?}");
        }
    }

    // Nothing leaked through: every master table is still empty.
    assert_eq!(live_count(&pool, "engagement_campaigns").await, 0);
    assert_eq!(live_count(&pool, "engagement_sources").await, 0);
    assert_eq!(live_count(&pool, "engagement_media").await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn boundary_length_values_pass_the_gate(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    let exactly_120 = "y".repeat(120);
    // Whitespace around a value is trimmed, not fatal.
    let refs = svc
        .resolve_attribution(
            &input(Some("  padded  "), Some(exactly_120.as_str()), None),
            None,
        )
        .await
        .expect("boundary values resolve");
    assert!(refs.campaign_id.is_some());
    assert!(refs.source_id.is_some());
}

// ─── authored vs minted campaigns ────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn minted_campaigns_carry_the_auto_marker_authored_do_not(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let minted = svc
        .resolve_attribution(&input(Some("Cookie Campaign"), None, None), None)
        .await
        .expect("mint");
    let authored = svc
        .create_campaign("Authored Campaign", None, None)
        .await
        .expect("author");

    let (minted_name, minted_auto) =
        campaign_row(&pool, minted.campaign_id.expect("minted id")).await;
    let (authored_name, authored_auto) =
        campaign_row(&pool, authored.campaign_id.expect("authored id")).await;

    assert_eq!(minted_name, "Cookie Campaign");
    assert!(minted_auto, "find-or-create mints must carry is_auto_campaign = true");
    assert_eq!(authored_name, "Authored Campaign");
    assert!(!authored_auto, "authored campaigns must carry is_auto_campaign = false");
}

// ─── the unique-name engine ──────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn name_counters_fill_the_first_free_slot(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let first = svc.create_campaign("Summer Sale", None, None).await.expect("first");
    let second = svc.create_campaign("Summer Sale", None, None).await.expect("second");
    let third = svc.create_campaign("Summer Sale", None, None).await.expect("third");

    assert_eq!(campaign_row(&pool, first.campaign_id.unwrap()).await.0, "Summer Sale");
    assert_eq!(campaign_row(&pool, second.campaign_id.unwrap()).await.0, "Summer Sale [1]");
    assert_eq!(campaign_row(&pool, third.campaign_id.unwrap()).await.0, "Summer Sale [2]");
}

#[sqlx::test(migrations = "./migrations")]
async fn a_counter_suffix_in_the_title_is_stripped_before_recounting(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let _ = svc.create_campaign("Summer Sale", None, None).await.expect("first");
    let _ = svc.create_campaign("Summer Sale", None, None).await.expect("second");
    // "Summer Sale [9]" normalizes to base "Summer Sale" (taken) and then
    // fills the first FREE counter — [1] is taken, so this lands on [2].
    let other = svc.create_campaign("Summer Sale [9]", None, None).await.expect("counter strip");
    assert_eq!(campaign_row(&pool, other.campaign_id.unwrap()).await.0, "Summer Sale [2]");
}

// ─── the authored path with an explicit identifier ───────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn explicit_identifiers_are_honored_and_collisions_refuse(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());

    let made = svc
        .create_campaign("Winter Push", Some("WS-2026"), None)
        .await
        .expect("explicit create");
    assert_eq!(campaign_row(&pool, made.campaign_id.unwrap()).await.0, "WS-2026");

    // Case-insensitive collision on the explicit identifier is a typed
    // refusal, never a silent counter.
    let err = svc
        .create_campaign("Another Winter", Some("ws-2026"), None)
        .await
        .expect_err("collision must refuse");
    assert!(matches!(err, EngagementError::CampaignNameTaken));

    // A different explicit identifier still works.
    let other = svc
        .create_campaign("Spring Push", Some("SP-2027"), None)
        .await
        .expect("distinct explicit create");
    assert_eq!(campaign_row(&pool, other.campaign_id.unwrap()).await.0, "SP-2027");
}

#[sqlx::test(migrations = "./migrations")]
async fn explicit_identifier_colliding_with_an_engine_minted_name_refuses(pool: PgPool) {
    let svc = EngagementWriteService::new(pool.clone());
    // The engine minted "Summer Sale" already.
    let _ = svc.create_campaign("Summer Sale", None, None).await.expect("engine mint");
    let err = svc
        .create_campaign("Explicit Clone", Some("summer sale"), None)
        .await
        .expect_err("engine-name collision must refuse");
    assert!(matches!(err, EngagementError::CampaignNameTaken));
}
