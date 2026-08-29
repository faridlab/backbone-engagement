//! Rating write-path cases (hand-written; user-owned).
//!
//! Exercises [`RatingWriteService`] against a real Postgres: idempotent
//! issuance for identified raters, the single-use capability submit (MAC
//! verify, expiry, tamper), the rotate/reset lifecycle, the publisher
//! reply, and the derived stats bands. Each test runs on its own scratch
//! database (`sqlx::test`) with the module migrations applied.
//!
//! The HMAC secret is fixed per test (explicit composition); the rated
//! type registry registers one parented pair — the shape a host composes.

use backbone_engagement::application::service::rating_write_service::{
    RatedTypeRegistry, RatingError, RatingWriteService,
};
use sqlx::PgPool;
use uuid::Uuid;

const SECRET: &[u8] = b"rating-case-secret";

/// A service with `helpdesk.ticket` registered under parent
/// `helpdesk.queue`.
fn service(pool: &PgPool) -> RatingWriteService {
    let mut registry = RatedTypeRegistry::new();
    registry.register("helpdesk.ticket", Some("helpdesk.queue"));
    RatingWriteService::with_config(pool.clone(), SECRET, registry)
}

/// A service with the rated type registered but NO secret (the
/// misconfigured-composition shape: the registry check passes and the
/// secret refusal is what fires).
fn bare_service(pool: &PgPool) -> RatingWriteService {
    let mut registry = RatedTypeRegistry::new();
    registry.register("helpdesk.ticket", None);
    RatingWriteService::with_config(pool.clone(), &[], registry)
}

/// One consumed rating row, seeded straight into the table (raw SQL is a
/// test-only affordance; the service never does this).
#[allow(clippy::too_many_arguments)]
async fn seed_consumed(
    pool: &PgPool,
    rated_model: &str,
    rated_record_id: Uuid,
    parent_record: Option<Uuid>,
    value: i32,
) {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO engagement.engagement_ratings
               (id, rated_model, rated_record_id, parent_rated_model,
                parent_rated_record_id, rating_value, consumed, rated_on,
                token_nonce, token_expires_at)
           VALUES ($1, $2, $3, $4, $5, $6, true, now(), $7, now() + interval '30 days')"#,
    )
    .bind(id)
    .bind(rated_model)
    .bind(rated_record_id)
    .bind(parent_record.map(|_| "helpdesk.queue".to_string()))
    .bind(parent_record)
    .bind(value)
    .bind(format!("seed-{id}"))
    .execute(pool)
    .await
    .expect("seed consumed rating");
}

// ─── issuance ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn issue_is_idempotent_for_identified_raters(pool: PgPool) {
    let svc = service(&pool);
    let record = Uuid::new_v4();
    let rater = Uuid::new_v4();

    let first = svc
        .issue_rating("helpdesk.ticket", record, Some(rater), None, None, None, None)
        .await
        .expect("first issue");
    assert!(first.created);

    // The live unconsumed row is reused: same id, no new row, and the
    // nonce survives (the already-sent link stays valid).
    let second = svc
        .issue_rating("helpdesk.ticket", record, Some(rater), None, None, None, None)
        .await
        .expect("second issue");
    assert_eq!(first.id, second.id);
    assert!(!second.created);
    assert_eq!(first.link, second.link);

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM engagement.engagement_ratings",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(rows, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymous_issuance_always_mints(pool: PgPool) {
    let svc = service(&pool);
    let record = Uuid::new_v4();

    let a = svc
        .issue_rating("helpdesk.ticket", record, None, Some("a@example.com"), None, None, None)
        .await
        .expect("anonymous issue a");
    let b = svc
        .issue_rating("helpdesk.ticket", record, None, Some("b@example.com"), None, None, None)
        .await
        .expect("anonymous issue b");

    // The idempotency index only covers identified raters — every
    // anonymous request is its own row (survey blasts mint per invitee).
    assert_ne!(a.id, b.id);
    assert!(a.created && b.created);
}

#[sqlx::test(migrations = "./migrations")]
async fn unregistered_rated_type_refused(pool: PgPool) {
    let svc = service(&pool);
    let err = svc
        .issue_rating("crm.lead", Uuid::new_v4(), Some(Uuid::new_v4()), None, None, None, None)
        .await
        .expect_err("unregistered model must refuse");
    assert!(matches!(err, RatingError::RatedTypeUnknown(_)));
    assert_eq!(err.http_status(), 422);
}

#[sqlx::test(migrations = "./migrations")]
async fn missing_secret_refuses_loudly(pool: PgPool) {
    let svc = bare_service(&pool);
    let err = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect_err("empty secret must refuse");
    assert!(matches!(err, RatingError::SecretNotConfigured));
    assert_eq!(err.http_status(), 500);
}

#[sqlx::test(migrations = "./migrations")]
async fn parent_pair_denormalizes_from_registry(pool: PgPool) {
    let svc = service(&pool);
    let record = Uuid::new_v4();
    let parent = Uuid::new_v4();

    let view = svc
        .issue_rating("helpdesk.ticket", record, None, None, None, Some(parent), None)
        .await
        .expect("issue with parent");

    let (pm, pr): (Option<String>, Option<Uuid>) = sqlx::query_as(
        "SELECT parent_rated_model, parent_rated_record_id FROM engagement.engagement_ratings WHERE id = $1",
    )
    .bind(view.id)
    .fetch_one(&pool)
    .await
    .expect("fetch row");
    assert_eq!(pm.as_deref(), Some("helpdesk.queue"));
    assert_eq!(pr, Some(parent));
}

// ─── submit (the single-use capability) ───────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn submit_consumes_exactly_once(pool: PgPool) {
    let svc = service(&pool);
    let view = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue");

    let id = svc
        .submit_rating(&view.link, 9, Some(" great "))
        .await
        .expect("first submit");

    let (consumed, value, feedback, rated_on): (bool, Option<i32>, Option<String>, Option<chrono::DateTime<chrono::Utc>>) =
        sqlx::query_as(
            "SELECT consumed, rating_value, feedback, rated_on FROM engagement.engagement_ratings WHERE id = $1",
        )
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("fetch consumed row");
    assert!(consumed);
    assert_eq!(value, Some(9));
    assert_eq!(feedback.as_deref(), Some("great")); // trimmed
    assert!(rated_on.is_some());

    // Second submit of the SAME link: the shared refusal, one shape.
    let err = svc
        .submit_rating(&view.link, 5, None)
        .await
        .expect_err("reuse must refuse");
    assert!(matches!(err, RatingError::NotSubmittable));
    assert_eq!(err.http_status(), 409);
}

#[sqlx::test(migrations = "./migrations")]
async fn submit_enforces_the_scale(pool: PgPool) {
    let svc = service(&pool);
    let low = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue low");
    let high = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue high");
    let edge_low = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue edge low");
    let edge_high = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue edge high");

    for (link, value) in [(&low.link, 0), (&high.link, 11)] {
        let err = svc
            .submit_rating(link, value, None)
            .await
            .expect_err("out of range");
        assert!(matches!(err, RatingError::RatingOutOfRange));
    }
    // The 1..=10 bounds themselves are accepted (the recorded deviation
    // from the upstream forced {1,3,5} scale).
    svc.submit_rating(&edge_low.link, 1, None).await.expect("value 1 accepted");
    svc.submit_rating(&edge_high.link, 10, None).await.expect("value 10 accepted");
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_capability_refused(pool: PgPool) {
    let svc = service(&pool);
    let view = svc
        .issue_rating(
            "helpdesk.ticket",
            Uuid::new_v4(),
            None,
            None,
            None,
            None,
            Some(-1), // expired a day ago
        )
        .await
        .expect("issue expired");

    let err = svc
        .submit_rating(&view.link, 8, None)
        .await
        .expect_err("expired must refuse");
    // Same shared shape as unknown/consumed — no expiry oracle.
    assert!(matches!(err, RatingError::NotSubmittable));
}

#[sqlx::test(migrations = "./migrations")]
async fn tampered_capability_refused(pool: PgPool) {
    let svc = service(&pool);
    let view = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, None, None, None)
        .await
        .expect("issue");

    // Flip the last MAC byte: verification recomputes over the STORED
    // fields, so a forged tag cannot ride a valid selector.
    let mut tampered = view.link.clone();
    let last = tampered.pop().expect("mac byte");
    tampered.push(if last == 'a' { 'b' } else { 'a' });
    let err = svc
        .submit_rating(&tampered, 8, None)
        .await
        .expect_err("tampered mac");
    assert!(matches!(err, RatingError::NotSubmittable));

    // A forged nonce under a VALID mac is equally dead: the mac input
    // covers the nonce, so re-signing is required and the secret is not
    // public.
    let err = svc
        .submit_rating("not-a-token", 8, None)
        .await
        .expect_err("garbage link");
    assert!(matches!(err, RatingError::NotSubmittable | RatingError::TokenMalformed));
}

// ─── rotate / reset ───────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn rotate_kills_the_old_link(pool: PgPool) {
    let svc = service(&pool);
    let view = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), Some(Uuid::new_v4()), None, None, None, None)
        .await
        .expect("issue");

    let rotated = svc.rotate_token(view.id, Some(7)).await.expect("rotate");
    assert_ne!(view.link, rotated.link);
    svc.submit_rating(&rotated.link, 7, None).await.expect("new link submits");

    let err = svc
        .submit_rating(&view.link, 7, None)
        .await
        .expect_err("old nonce is dead");
    assert!(matches!(err, RatingError::NotSubmittable));
}

#[sqlx::test(migrations = "./migrations")]
async fn reset_clears_the_answer_and_re_arms(pool: PgPool) {
    let svc = service(&pool);
    let record = Uuid::new_v4();
    let view = svc
        .issue_rating("helpdesk.ticket", record, Some(Uuid::new_v4()), None, None, None, None)
        .await
        .expect("issue");
    svc.submit_rating(&view.link, 2, Some("first answer")).await.expect("first submit");

    // The reuse wave: answer cleared, capability re-armed with a NEW nonce.
    let reset = svc.reset_rating(view.id, None).await.expect("reset");
    let (consumed, value, feedback): (bool, Option<i32>, Option<String>) = sqlx::query_as(
        "SELECT consumed, rating_value, feedback FROM engagement.engagement_ratings WHERE id = $1",
    )
    .bind(view.id)
    .fetch_one(&pool)
    .await
    .expect("fetch reset row");
    assert!(!consumed);
    assert_eq!(value, None);
    assert_eq!(feedback, None);

    // Stats dropped to zero (the old answer no longer counts)…
    let stats = svc.record_stats("helpdesk.ticket", record).await.expect("stats after reset");
    assert_eq!(stats.total, 0);
    // …and the fresh link carries the corrected answer.
    svc.submit_rating(&reset.link, 9, None).await.expect("re-submit");
    let stats = svc.record_stats("helpdesk.ticket", record).await.expect("stats after re-submit");
    assert_eq!(stats.total, 1);
    assert_eq!(stats.average, 9.0);
}

// ─── publisher reply ──────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn publisher_reply_writes_the_whole_trio(pool: PgPool) {
    let svc = service(&pool);
    let view = svc
        .issue_rating("helpdesk.ticket", Uuid::new_v4(), None, None, Some(Uuid::new_v4()), None, None)
        .await
        .expect("issue");
    let publisher = Uuid::new_v4();

    svc.publisher_reply(view.id, "  thanks for the feedback  ", publisher)
        .await
        .expect("reply");

    let (comment, uid, at): (String, Uuid, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT publisher_comment, publisher_user_id, publisher_replied_at FROM engagement.engagement_ratings WHERE id = $1",
    )
    .bind(view.id)
    .fetch_one(&pool)
    .await
    .expect("fetch reply");
    assert_eq!(comment, "thanks for the feedback"); // trimmed
    assert_eq!(uid, publisher); // forced server-side, not caller-chosen
    assert!(at.is_some());

    // An empty reply is a validation refusal, not a row write.
    let err = svc
        .publisher_reply(view.id, "   ", publisher)
        .await
        .expect_err("empty reply");
    assert_eq!(err.http_status(), 422);
}

// ─── stats (the derived bands) ────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn stats_bands_roll_up_the_scale(pool: PgPool) {
    let svc = service(&pool);
    let record = Uuid::new_v4();
    for value in [9, 6, 3] {
        seed_consumed(&pool, "helpdesk.ticket", record, None, value).await;
    }
    // An unconsumed request is invisible to stats.
    svc.issue_rating("helpdesk.ticket", record, None, None, None, None, None)
        .await
        .expect("issue unconsumed");

    let stats = svc.record_stats("helpdesk.ticket", record).await.expect("stats");
    assert_eq!(stats.total, 3);
    assert_eq!(stats.top, 1); // 9 >= 8
    assert_eq!(stats.ok, 1); // 6 in 5..=7
    assert_eq!(stats.ko, 1); // 3 <= 4
    assert_eq!(stats.percentage_satisfaction, 33); // 1/3 rounded
    assert!((stats.average - 6.0).abs() < f64::EPSILON);
}

#[sqlx::test(migrations = "./migrations")]
async fn parent_rollup_aggregates_children(pool: PgPool) {
    let svc = service(&pool);
    let parent = Uuid::new_v4();
    let child_a = Uuid::new_v4();
    let child_b = Uuid::new_v4();

    seed_consumed(&pool, "helpdesk.ticket", child_a, Some(parent), 10).await;
    seed_consumed(&pool, "helpdesk.ticket", child_a, Some(parent), 8).await;
    seed_consumed(&pool, "helpdesk.ticket", child_b, Some(parent), 4).await;
    // A child with a different parent must stay out of this rollup.
    seed_consumed(&pool, "helpdesk.ticket", Uuid::new_v4(), Some(Uuid::new_v4()), 10).await;

    let stats = svc
        .parent_stats("helpdesk.queue", parent)
        .await
        .expect("parent stats");
    assert_eq!(stats.total, 3);
    assert_eq!(stats.top, 2);
    assert_eq!(stats.ok, 0);
    assert_eq!(stats.ko, 1);
    assert_eq!(stats.percentage_satisfaction, 67); // 2/3 rounded
}
