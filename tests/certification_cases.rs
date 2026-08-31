//! Certification badge-contract cases (hand-written; user-owned).
//!
//! The inbound `CertificationPassed` fact is the survey certification
//! contract: the producing module publishes the fact (through its own
//! fail-closed port), a host adapter delivers it here, and this module
//! mints the badge exactly once per (certification, attempt). These cases
//! pin the contract's observables against a real Postgres, each on its own
//! scratch database (`sqlx::test`) with the module migrations applied:
//!
//! - exactly-once under sequential replay AND under concurrent duplicate
//!   delivery (the `grant_key` partial UNIQUE converges every racer on one
//!   row);
//! - negative payloads refuse loudly and mint nothing;
//! - the peer-grant self-refusal and the append-only karma ledger are
//!   unaffected by the certification path (a system grant is a different
//!   keyed verb, not a ladder bypass);
//! - an unregistered badge key is a typed refusal, never a silent drop.
//!
//! Raw SQL in here is assertion/fixture-only — the service layer never
//! touches sqlx directly.

use std::collections::HashSet;

use backbone_engagement::application::service::gamification_write_service::{
    CertificationPassedFact, GamificationError, GamificationWriteService,
};
use sqlx::PgPool;
use uuid::Uuid;

fn bare_service(pool: &PgPool) -> GamificationWriteService {
    GamificationWriteService::new(pool.clone())
}

async fn insert_badge(pool: &PgPool, name: &str) -> Uuid {
    sqlx::query_scalar(
        r#"INSERT INTO engagement.gamification_badges
               (name, active, level, rule_auth, rule_auth_user_ids,
                rule_auth_badge_ids, rule_max, rule_max_number)
           VALUES ($1, true, NULL, 'nobody', '[]'::jsonb, '[]'::jsonb, false, NULL)
           RETURNING id"#,
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("insert badge")
}

/// One well-formed fact: `survey:{uuid}` certification ref, distinct
/// attempt, a real recipient.
fn fact(certification_ref: &str, attempt: &str, recipient: Uuid, badge_key: &str) -> CertificationPassedFact {
    CertificationPassedFact {
        certification_ref: certification_ref.to_string(),
        survey_ref: Some(Uuid::new_v4()),
        attempt_ref: attempt.to_string(),
        recipient_user_id: recipient,
        badge_key: badge_key.to_string(),
    }
}

async fn grant_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM engagement.gamification_badge_users")
        .fetch_one(pool)
        .await
        .expect("grant count")
}

async fn ledger_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM engagement.gamification_karma_trackings")
        .fetch_one(pool)
        .await
        .expect("ledger count")
}

// ─── key derivation (pure) ─────────────────────────────────────────────────────

#[test]
fn grant_key_derivation_is_the_contract_grain() {
    let f = fact("survey:11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222", Uuid::new_v4(), "certified-pro");
    assert_eq!(
        f.grant_key(),
        "event:certification:survey:11111111-1111-1111-1111-111111111111:22222222-2222-2222-2222-222222222222"
    );
    // The grain is (certification, attempt): a different attempt differs.
    let other = fact("survey:11111111-1111-1111-1111-111111111111", "33333333-3333-3333-3333-333333333333", Uuid::new_v4(), "certified-pro");
    assert_ne!(f.grant_key(), other.grant_key());
    // And a different certification differs for the same attempt.
    let other_cert = fact("survey:99999999-9999-9999-9999-999999999999", "22222222-2222-2222-2222-222222222222", Uuid::new_v4(), "certified-pro");
    assert_ne!(f.grant_key(), other_cert.grant_key());
    // A well-formed fact passes shape validation with no database at all.
    f.validate().expect("well-formed fact validates");
}

#[test]
fn payload_validation_refuses_malformed_facts_purely() {
    let recipient = Uuid::new_v4();
    let base = fact("survey:abc", "attempt-1", recipient, "certified-pro");

    let mut empty_cert = base.clone();
    empty_cert.certification_ref = String::new();
    assert!(matches!(
        empty_cert.validate(),
        Err(GamificationError::CertificationPayloadInvalid(_))
    ));

    let mut blank_cert = base.clone();
    blank_cert.certification_ref = "   ".to_string();
    assert!(matches!(
        blank_cert.validate(),
        Err(GamificationError::CertificationPayloadInvalid(_))
    ));

    let mut empty_attempt = base.clone();
    empty_attempt.attempt_ref = String::new();
    assert!(matches!(
        empty_attempt.validate(),
        Err(GamificationError::CertificationPayloadInvalid(_))
    ));

    let mut empty_badge = base.clone();
    empty_badge.badge_key = "  ".to_string();
    assert!(matches!(
        empty_badge.validate(),
        Err(GamificationError::CertificationPayloadInvalid(_))
    ));

    let mut oversize = base.clone();
    oversize.certification_ref = "x".repeat(201);
    assert!(matches!(
        oversize.validate(),
        Err(GamificationError::CertificationPayloadInvalid(_))
    ));
}

// ─── exactly-once: sequential replay ───────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn certification_grants_once_and_converges_on_replay(pool: PgPool) {
    let svc = bare_service(&pool);
    let badge = insert_badge(&pool, "certified-pro").await;
    let user = Uuid::new_v4();
    let f = fact("survey:s-1", "attempt-1", user, "certified-pro");

    let first = svc.certification_passed(&f).await.expect("first delivery grants");
    assert!(first.created);
    assert_eq!(first.badge_id, badge);
    assert_eq!(first.recipient_user_id, user);
    assert_eq!(first.grant_key, f.grant_key());

    // Redelivery of the SAME fact converges on the same row.
    let replay = svc.certification_passed(&f).await.expect("replay converges");
    assert!(!replay.created);
    assert_eq!(first.grant_row_id, replay.grant_row_id);
    assert_eq!(grant_count(&pool).await, 1);

    // A new attempt is a NEW fact — one more row (the producer publishes
    // once per first pool success; a retake is a distinct idempotency
    // grain).
    let retake = fact("survey:s-1", "attempt-2", user, "certified-pro");
    let second = svc.certification_passed(&retake).await.expect("retake grants");
    assert!(second.created);
    assert_eq!(grant_count(&pool).await, 2);

    // The event kind and the NULL challenge coupling hold on every row.
    let rows: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT grant_kind::text, challenge_id FROM engagement.gamification_badge_users",
    )
    .fetch_all(&pool)
    .await
    .expect("rows");
    assert!(rows.iter().all(|(kind, challenge)| kind == "event" && challenge.is_none()));
}

// ─── exactly-once: concurrent duplicate delivery ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn certification_is_exactly_once_under_concurrent_duplicates(pool: PgPool) {
    let svc = std::sync::Arc::new(bare_service(&pool));
    insert_badge(&pool, "certified-pro").await;
    let f = std::sync::Arc::new(fact("survey:s-race", "attempt-race", Uuid::new_v4(), "certified-pro"));

    // Eight overlapping deliveries of the same fact. The partial UNIQUE
    // index plus ON CONFLICT DO NOTHING converge every racer on ONE row;
    // the losers resolve by reading the winner's row.
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let svc = svc.clone();
            let f = f.clone();
            tokio::spawn(async move { svc.certification_passed(&f).await })
        })
        .collect();
    let mut views = Vec::with_capacity(handles.len());
    for h in handles {
        views.push(h.await.expect("join").expect("every delivery resolves"));
    }

    assert_eq!(grant_count(&pool).await, 1, "exactly one grant row may exist");
    assert_eq!(
        views.iter().filter(|v| v.created).count(),
        1,
        "exactly one delivery reports created"
    );
    let row_ids: HashSet<Uuid> = views.iter().map(|v| v.grant_row_id).collect();
    assert_eq!(row_ids.len(), 1, "every delivery converges on the same row");
    assert!(views.iter().all(|v| v.grant_key == f.grant_key()));
}

// ─── negative payloads: refuse loudly, mint nothing ────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn negative_payloads_refuse_and_mint_nothing(pool: PgPool) {
    let svc = bare_service(&pool);
    insert_badge(&pool, "certified-pro").await;
    let user = Uuid::new_v4();

    for f in [
        fact("", "attempt-1", user, "certified-pro"),
        fact("   ", "attempt-1", user, "certified-pro"),
        fact("survey:s-1", "", user, "certified-pro"),
        fact("survey:s-1", "attempt-1", user, ""),
        fact("survey:s-1", "attempt-1", user, "   "),
        fact(&"x".repeat(201), "attempt-1", user, "certified-pro"),
        fact("survey:s-1", &"x".repeat(201), user, "certified-pro"),
        fact("survey:s-1", "attempt-1", user, &"k".repeat(201)),
    ] {
        let err = match svc.certification_passed(&f).await {
            Ok(_) => panic!("malformed fact must refuse: {f:?}"),
            Err(err) => err,
        };
        assert!(
            matches!(err, GamificationError::CertificationPayloadInvalid(_)),
            "expected a payload refusal, got: {err:?}"
        );
        assert_eq!(err.code(), "certification_payload_invalid");
        assert_eq!(err.http_status(), 422);
    }
    // Nothing was minted along the way.
    assert_eq!(grant_count(&pool).await, 0);

    // The badge-key resolution failure is its own typed refusal (a runtime
    // miss, not a shape error): 422, no row.
    let unknown = fact("survey:s-1", "attempt-1", user, "no-such-badge");
    let err = svc
        .certification_passed(&unknown)
        .await
        .expect_err("unknown badge key refuses");
    assert!(matches!(err, GamificationError::BadgeKeyUnknown(_)));
    assert_eq!(err.http_status(), 422);
    assert_eq!(grant_count(&pool).await, 0);

    // The argument-shaped alias routes through the same validation.
    let err = svc
        .on_certification_passed("", "attempt-1", user, "certified-pro", None)
        .await
        .expect_err("alias refuses malformed payloads too");
    assert!(matches!(err, GamificationError::CertificationPayloadInvalid(_)));
    assert_eq!(grant_count(&pool).await, 0);
}

// ─── the certification path leaves the ladder and the ledger intact ───────────

#[sqlx::test(migrations = "./migrations")]
async fn self_grant_refusal_and_ledger_immutability_survive_certification(pool: PgPool) {
    let svc = bare_service(&pool);
    // The certification badge is ladder-closed (nobody): ONLY the keyed
    // system verb can grant it.
    let badge = insert_badge(&pool, "certified-pro").await;
    let user = Uuid::new_v4();
    svc.append_karma(user, 25, Some("baseline"), "manual", None, None)
        .await
        .expect("seed ledger");
    let ledger_before = ledger_count(&pool).await;

    // The certification grant lands for the same user who would be refused
    // by the peer ladder...
    svc.certification_passed(&fact("survey:s-1", "attempt-1", user, "certified-pro"))
        .await
        .expect("system grant");

    // ...and the peer self-grant refusal is untouched by the new surface.
    let err = svc.grant_peer(badge, user, user, None).await.expect_err("self grant still refuses");
    assert!(matches!(err, GamificationError::BadgeSelfGrant));
    // The ladder's hard stop (nobody) still refuses every peer attempt.
    let err = svc
        .grant_peer(badge, Uuid::new_v4(), user, None)
        .await
        .expect_err("nobody ladder still refuses");
    assert!(matches!(err, GamificationError::BadgeNotGrantable(_)));

    // The certification verb performs NO ledger writes: the badge grant is
    // the only observable.
    assert_eq!(ledger_count(&pool).await, ledger_before);
    assert_eq!(grant_count(&pool).await, 1);

    // And the ledger stays append-only at the DB: UPDATE and DELETE on a
    // ledger row still raise the immutability trigger.
    let row: Uuid = sqlx::query_scalar(
        "SELECT id FROM engagement.gamification_karma_trackings WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("ledger row");
    let upd = sqlx::query(
        "UPDATE engagement.gamification_karma_trackings SET reason = 'forged' WHERE id = $1",
    )
    .bind(row)
    .execute(&pool)
    .await;
    assert!(upd.is_err(), "UPDATE must raise the immutability trigger");
    let del = sqlx::query("DELETE FROM engagement.gamification_karma_trackings WHERE id = $1")
        .bind(row)
        .execute(&pool)
        .await;
    assert!(del.is_err(), "DELETE must raise the immutability trigger");

    // Replay after all of this still converges (the contract never mints
    // twice, even interleaved with ladder refusals).
    let replay = svc
        .certification_passed(&fact("survey:s-1", "attempt-1", user, "certified-pro"))
        .await
        .expect("replay");
    assert!(!replay.created);
    assert_eq!(grant_count(&pool).await, 1);
}
