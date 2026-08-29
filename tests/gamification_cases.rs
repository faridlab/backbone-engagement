//! Gamification write-path cases (hand-written; user-owned).
//!
//! Exercises [`GamificationWriteService`] against a real Postgres: the
//! append-only karma ledger and its projection reads, the two badge-grant
//! verbs (the peer ladder with its cap and self-grant refusal; the keyed
//! system grant's exactly-once convergence), the certification inbound
//! contract, the metric-registry gate, both state machines, the leaver
//! sweep, and the daily challenge check. Each test runs on its own scratch
//! database (`sqlx::test`) with the module migrations applied.
//!
//! Raw SQL in here is assertion/fixture-only — the service layer never
//! touches sqlx directly.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use backbone_engagement::application::service::gamification_write_service::{
    GamificationError, GamificationWriteService,
};
use backbone_engagement::domain::entity::{
    BadgeGrantKind, GoalComputationMode, GoalCondition, GoalDisplayMode, GoalState,
};
use chrono::{Duration, NaiveDate, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

/// The balance projection orders ledger rows by `tracking_date`
/// (microsecond resolution). Sequential appends in one test can land
/// inside the same microsecond, so space them — production appends are
/// minutes apart.
async fn tick() {
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
}

/// A service with one registered metric (`test.sales`) backed by a shared
/// value map the test mutates — the "code, not data" compute seam.
fn metric_service(pool: &PgPool) -> (Arc<GamificationWriteService>, Arc<Mutex<HashMap<Uuid, i64>>>) {
    let values: Arc<Mutex<HashMap<Uuid, i64>>> = Arc::new(Mutex::new(HashMap::new()));
    let seen = values.clone();
    let mut svc = GamificationWriteService::new(pool.clone());
    svc.register_metric(
        "test.sales",
        Arc::new(move |user, _| Decimal::from(seen.lock().unwrap().get(&user).copied().unwrap_or(0))),
    );
    (Arc::new(svc), values)
}

/// A bare service (no metric, no directory).
fn bare_service(pool: &PgPool) -> Arc<GamificationWriteService> {
    Arc::new(GamificationWriteService::new(pool.clone()))
}

#[allow(clippy::too_many_arguments)]
async fn insert_badge(
    pool: &PgPool,
    name: &str,
    active: bool,
    level: Option<&str>,
    rule_auth: &str,
    rule_auth_user_ids: &str,
    rule_auth_badge_ids: &str,
    rule_max: bool,
    rule_max_number: Option<i32>,
) -> Uuid {
    sqlx::query_scalar(
        r#"INSERT INTO engagement.gamification_badges
               (name, active, level, rule_auth, rule_auth_user_ids,
                rule_auth_badge_ids, rule_max, rule_max_number)
           VALUES ($1, $2, $3::badge_level, $4::badge_rule_auth, $5::jsonb,
                   $6::jsonb, $7, $8)
           RETURNING id"#,
    )
    .bind(name)
    .bind(active)
    .bind(level)
    .bind(rule_auth)
    .bind(rule_auth_user_ids)
    .bind(rule_auth_badge_ids)
    .bind(rule_max)
    .bind(rule_max_number)
    .fetch_one(pool)
    .await
    .expect("insert badge")
}

impl Default for ChallengeFixture {
    fn default() -> Self {
        // Schema defaults mirrored: realtime rewards on, reports off.
        Self {
            name: String::new(),
            start: None,
            end: None,
            user_ids: Vec::new(),
            include_all: false,
            include_badge_ids: Vec::new(),
            realtime: true,
            reward_badge: None,
            first: None,
            second: None,
            third: None,
            failure: false,
            report: "never",
        }
    }
}

struct ChallengeFixture {
    name: String,
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
    user_ids: Vec<Uuid>,
    include_all: bool,
    include_badge_ids: Vec<Uuid>,
    realtime: bool,
    reward_badge: Option<Uuid>,
    first: Option<Uuid>,
    second: Option<Uuid>,
    third: Option<Uuid>,
    failure: bool,
    report: &'static str,
}

impl ChallengeFixture {
    fn named(name: &str) -> Self {
        Self { name: name.to_string(), realtime: true, ..Default::default() }
    }

    fn users(mut self, users: &[Uuid]) -> Self {
        self.user_ids = users.to_vec();
        self
    }

    fn span(mut self, start: NaiveDate, end: Option<NaiveDate>) -> Self {
        self.start = Some(start);
        self.end = end;
        self
    }

    async fn insert(self, pool: &PgPool) -> Uuid {
        let user_ids: Vec<String> = self.user_ids.iter().map(|u| format!("\"{u}\"")).collect();
        let badge_ids: Vec<String> =
            self.include_badge_ids.iter().map(|u| format!("\"{u}\"")).collect();
        sqlx::query_scalar(
            r#"INSERT INTO engagement.gamification_challenges
                   (name, start_date, end_date, reward_badge_id,
                    reward_first_badge_id, reward_second_badge_id,
                    reward_third_badge_id, reward_failure, reward_realtime,
                    report_frequency, user_ids, include_all_users, include_badge_ids)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::report_frequency,
                       $11::jsonb, $12, $13::jsonb)
               RETURNING id"#,
        )
        .bind(&self.name)
        .bind(self.start)
        .bind(self.end)
        .bind(self.reward_badge)
        .bind(self.first)
        .bind(self.second)
        .bind(self.third)
        .bind(self.failure)
        .bind(self.realtime)
        .bind(self.report)
        .bind(format!("[{}]", user_ids.join(",")))
        .bind(self.include_all)
        .bind(format!("[{}]", badge_ids.join(",")))
        .fetch_one(pool)
        .await
        .expect("insert challenge")
    }
}

async fn insert_line(pool: &PgPool, challenge: Uuid, definition: Uuid, target: i64) {
    sqlx::query(
        r#"INSERT INTO engagement.gamification_challenge_lines
               (challenge_id, definition_id, sequence, target_goal)
           VALUES ($1, $2, 1, $3)"#,
    )
    .bind(challenge)
    .bind(definition)
    .bind(Decimal::from(target))
    .execute(pool)
    .await
    .expect("insert line");
}

async fn challenge_state(pool: &PgPool, id: Uuid) -> String {
    sqlx::query_scalar("SELECT state::text FROM engagement.gamification_challenges WHERE id = $1")
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("challenge state")
}

async fn goal_states(pool: &PgPool, challenge: Uuid) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT state::text FROM engagement.gamification_goals WHERE challenge_id = $1 ORDER BY id",
    )
    .bind(challenge)
    .fetch_all(pool)
    .await
    .expect("goal states")
}

async fn grant_count(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM engagement.gamification_badge_users")
        .fetch_one(pool)
        .await
        .expect("grant count")
}

async fn insert_rank(pool: &PgPool, name: &str, karma_min: i32) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO engagement.gamification_karma_ranks (name, karma_min) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(karma_min)
    .fetch_one(pool)
    .await
    .expect("insert rank")
}

// ─── the karma ledger (append-only) ───────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn ledger_appends_and_balance_projects(pool: PgPool) {
    let svc = bare_service(&pool);
    let user = Uuid::new_v4();

    let a = svc.append_karma(user, 50, Some("onboarding"), "manual", None, None).await.expect("append 1");
    tick().await;
    let b = svc.append_karma(user, 30, Some("sale"), "manual", None, None).await.expect("append 2");
    tick().await;
    let c = svc.append_karma(user, -20, Some("refund"), "host", None, None).await.expect("append 3");

    // The chain: old→new per row, balance = the LATEST row (never a sum).
    assert_eq!((a.old_value, a.new_value), (0, 50));
    assert_eq!((b.old_value, b.new_value), (50, 80));
    assert_eq!((c.old_value, c.new_value), (80, 60));
    assert_eq!(svc.balance(user).await.expect("balance"), 60);

    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM engagement.gamification_karma_trackings WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("ledger rows");
    assert_eq!(rows, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn ledger_rejects_update_and_delete_by_trigger(pool: PgPool) {
    let svc = bare_service(&pool);
    let user = Uuid::new_v4();
    svc.append_karma(user, 10, None, "manual", None, None).await.expect("append");

    let row: Uuid = sqlx::query_scalar(
        "SELECT id FROM engagement.gamification_karma_trackings WHERE user_id = $1",
    )
    .bind(user)
    .fetch_one(&pool)
    .await
    .expect("row");

    // The DB backstop behind the repository's no-update/no-delete promise:
    // both mutations raise, soft-delete included.
    let upd = sqlx::query("UPDATE engagement.gamification_karma_trackings SET reason = 'forged' WHERE id = $1")
        .bind(row)
        .execute(&pool)
        .await;
    assert!(upd.is_err(), "UPDATE must raise the immutability trigger");

    let del = sqlx::query("DELETE FROM engagement.gamification_karma_trackings WHERE id = $1")
        .bind(row)
        .execute(&pool)
        .await;
    assert!(del.is_err(), "DELETE must raise the immutability trigger");

    let soft = sqlx::query(
        "UPDATE engagement.gamification_karma_trackings SET metadata = metadata || '{\"deleted_at\":\"2026-01-01\"}'::jsonb WHERE id = $1",
    )
    .bind(row)
    .execute(&pool)
    .await;
    assert!(soft.is_err(), "soft-delete must equally raise");
}

#[sqlx::test(migrations = "./migrations")]
async fn append_validates_the_origin_vocabulary(pool: PgPool) {
    let svc = bare_service(&pool);
    let err = svc
        .append_karma(Uuid::new_v4(), 5, None, "mystery", None, None)
        .await
        .expect_err("unknown origin");
    assert!(matches!(err, GamificationError::OriginKindUnknown(_)));
    assert_eq!(err.http_status(), 422);
}

#[sqlx::test(migrations = "./migrations")]
async fn leaderboard_ranks_the_balance_projection(pool: PgPool) {
    let svc = bare_service(&pool);
    let (a, b, c) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());

    svc.append_karma(a, 100, None, "manual", None, None).await.expect("a");
    tick().await;
    svc.append_karma(b, 200, None, "manual", None, None).await.expect("b");
    tick().await;
    svc.append_karma(c, 50, None, "manual", None, None).await.expect("c");
    // A later append REBASES a's projection — the leaderboard ranks the
    // latest row, not the sum (the dropped upstream window semantics).
    svc.append_karma(a, 300, None, "manual", None, None).await.expect("a again");
    tick().await;

    let board = svc.leaderboard(Some(10)).await.expect("leaderboard");
    assert_eq!(board.len(), 3);
    assert_eq!(board[0].user_id, a);
    assert_eq!(board[0].balance, 400); // latest row, not 100+300-as-sum… same value here — see next assert
    assert_eq!(board[1].user_id, b);
    assert_eq!(board[2].user_id, c);
    // Negative rebase: the projection can DROP without any consolidation.
    svc.append_karma(c, -100, None, "host", None, None).await.expect("c rebase");
    tick().await;
    let board = svc.leaderboard(Some(10)).await.expect("leaderboard 2");
    assert_eq!(board[2].balance, -50);
}

#[sqlx::test(migrations = "./migrations")]
async fn rank_ladder_and_threshold_check(pool: PgPool) {
    let svc = bare_service(&pool);
    insert_rank(&pool, "Bronze", 1).await;
    let silver = insert_rank(&pool, "Silver", 100).await;
    let gold = insert_rank(&pool, "Gold", 500).await;

    let (current, next) = svc.rank_for_balance(150).await.expect("rank walk");
    assert_eq!(current.expect("silver").id, silver);
    assert_eq!(next.expect("gold").id, gold);

    // karma_min > 0 is a table CHECK — the ladder cannot start at zero.
    let bad = sqlx::query(
        "INSERT INTO engagement.gamification_karma_ranks (name, karma_min) VALUES ('Ghost', 0)",
    )
    .execute(&pool)
    .await;
    assert!(bad.is_err(), "karma_min must be strictly positive");

    // A rank-crossing append reports the change fact.
    let view = svc
        .append_karma(Uuid::new_v4(), 120, None, "manual", None, None)
        .await
        .expect("cross");
    assert!(view.from_rank.is_none());
    assert_eq!(view.to_rank.expect("to rank").id, silver);

    let view = svc
        .append_karma(Uuid::new_v4(), 600, None, "manual", None, None)
        .await
        .expect("straight to gold");
    assert_eq!(view.to_rank.expect("top").id, gold);
}

// ─── badges: the peer ladder ──────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn peer_grant_snapshots_level_and_respects_ladder(pool: PgPool) {
    let svc = bare_service(&pool);
    let (sender, outsider, friend) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());

    // everyone: any identified sender; the badge's level snapshot lands on
    // the grant row.
    let open = insert_badge(&pool, "helper", true, Some("gold"), "everyone", "[]", "[]", false, None).await;
    let view = svc.grant_peer(open, sender, friend, Some(" thank you ")).await.expect("everyone grant");
    assert_eq!(view.grant_kind, BadgeGrantKind::Peer);
    let (level, comment): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT level::text, comment FROM engagement.gamification_badge_users WHERE id = $1",
    )
    .bind(view.grant_row_id)
    .fetch_one(&pool)
    .await
    .expect("grant row");
    assert_eq!(level.as_deref(), Some("gold"));
    assert_eq!(comment.as_deref(), Some("thank you"));

    // nobody: the ladder's hard stop — no admin bypass verb exists.
    let closed = insert_badge(&pool, "founder", true, None, "nobody", "[]", "[]", false, None).await;
    let err = svc.grant_peer(closed, sender, friend, None).await.expect_err("nobody refuses");
    assert!(matches!(err, GamificationError::BadgeNotGrantable(_)));

    // users: the named-allowlist arm.
    let allow = serde_json::to_string(&vec![sender]).unwrap();
    let gated = insert_badge(&pool, "mentor", true, None, "users", &allow, "[]", false, None).await;
    svc.grant_peer(gated, sender, friend, None).await.expect("allowlisted sender");
    let err = svc.grant_peer(gated, outsider, friend, None).await.expect_err("outsider refuses");
    assert!(matches!(err, GamificationError::BadgeNotGrantable(_)));

    // having: the sender must HOLD every prerequisite badge.
    let prereq = insert_badge(&pool, "rookie", true, None, "everyone", "[]", "[]", false, None).await;
    svc.grant_peer(prereq, sender, sender, None).await.expect_err("self grant on prereq");
    svc.grant_system(prereq, sender, BadgeGrantKind::Event, "event:seed:1", None, None)
        .await
        .expect("seed prereq hold");
    let prereq_json = serde_json::to_string(&vec![prereq]).unwrap();
    let advanced = insert_badge(&pool, "veteran", true, None, "having", "[]", &prereq_json, false, None).await;
    svc.grant_peer(advanced, sender, friend, None).await.expect("holder grants");
    let err = svc.grant_peer(advanced, outsider, friend, None).await.expect_err("non-holder refuses");
    assert!(matches!(err, GamificationError::BadgeNotGrantable(_)));
}

#[sqlx::test(migrations = "./migrations")]
async fn peer_grant_refuses_self_and_inactive(pool: PgPool) {
    let svc = bare_service(&pool);
    let user = Uuid::new_v4();
    let badge = insert_badge(&pool, "helper", true, None, "everyone", "[]", "[]", false, None).await;

    let err = svc.grant_peer(badge, user, user, None).await.expect_err("self grant");
    assert!(matches!(err, GamificationError::BadgeSelfGrant));
    assert_eq!(err.http_status(), 422);

    let retired = insert_badge(&pool, "legacy", false, None, "everyone", "[]", "[]", false, None).await;
    let err = svc.grant_peer(retired, Uuid::new_v4(), user, None).await.expect_err("inactive badge");
    assert!(matches!(err, GamificationError::BadgeInactive));
}

#[sqlx::test(migrations = "./migrations")]
async fn peer_grant_monthly_cap(pool: PgPool) {
    let svc = bare_service(&pool);
    let sender = Uuid::new_v4();
    let badge = insert_badge(&pool, "kudos", true, None, "everyone", "[]", "[]", true, Some(2)).await;

    let r1 = Uuid::new_v4();
    let r2 = Uuid::new_v4();
    svc.grant_peer(badge, sender, r1, None).await.expect("grant 1");
    svc.grant_peer(badge, sender, r2, None).await.expect("grant 2");

    let err = svc.grant_peer(badge, sender, Uuid::new_v4(), None).await.expect_err("cap reached");
    assert!(matches!(err, GamificationError::BadgeCapReached));
    assert_eq!(err.http_status(), 422);

    // The cap is per (badge, sender, month): another sender is unaffected.
    svc.grant_peer(badge, Uuid::new_v4(), Uuid::new_v4(), None).await.expect("other sender");

    let stats = svc.badge_stats(badge).await.expect("stats");
    assert_eq!(stats.granted_count, 3);
    assert_eq!(stats.granted_users_count, 3);
    assert_eq!(stats.granted_this_month, 3);
}

// ─── badges: the keyed system grant (exactly-once) ────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn system_grant_is_exactly_once_sequentially(pool: PgPool) {
    let svc = bare_service(&pool);
    let badge = insert_badge(&pool, "cert", true, Some("silver"), "nobody", "[]", "[]", false, None).await;
    let user = Uuid::new_v4();

    let first = svc
        .grant_system(badge, user, BadgeGrantKind::Event, "event:cert:c1:a1", None, None)
        .await
        .expect("first");
    assert!(first.created);
    let second = svc
        .grant_system(badge, user, BadgeGrantKind::Event, "event:cert:c1:a1", None, None)
        .await
        .expect("replay converges");
    assert!(!second.created);
    assert_eq!(first.grant_row_id, second.grant_row_id); // same row
    assert_eq!(grant_count(&pool).await, 1);

    // A peer-kind system grant is a category error — the verb exists for
    // challenge/event only.
    let err = svc
        .grant_system(badge, user, BadgeGrantKind::Peer, "peer:weird", None, None)
        .await
        .expect_err("peer kind refused");
    assert!(matches!(err, GamificationError::GrantKindInvalid));
}

#[sqlx::test(migrations = "./migrations")]
async fn system_grant_is_exactly_once_under_concurrency(pool: PgPool) {
    let svc = bare_service(&pool);
    let badge = insert_badge(&pool, "cert", true, None, "nobody", "[]", "[]", false, None).await;
    let user = Uuid::new_v4();
    let key = "event:cert:race:1";

    // Six overlapping deliveries of the same fact: the partial UNIQUE
    // index + ON CONFLICT DO NOTHING converge them all on ONE row.
    let s1 = svc.clone();
    let s2 = svc.clone();
    let s3 = svc.clone();
    let s4 = svc.clone();
    let s5 = svc.clone();
    let (a, b, c, d, e, f) = tokio::join!(
        async { s1.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
        async { s2.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
        async { s3.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
        async { s4.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
        async { s5.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
        async { svc.grant_system(badge, user, BadgeGrantKind::Event, key, None, None).await },
    );
    let views = [a, b, c, d, e, f]
        .into_iter()
        .map(|v| v.expect("every concurrent delivery resolves"))
        .collect::<Vec<_>>();

    assert_eq!(grant_count(&pool).await, 1, "exactly one row may exist");
    let created: usize = views.iter().filter(|v| v.created).count();
    assert_eq!(created, 1, "exactly one delivery reports created");
    let row_ids: std::collections::HashSet<Uuid> =
        views.iter().map(|v| v.grant_row_id).collect();
    assert_eq!(row_ids.len(), 1, "every delivery converges on the same row");
}

#[sqlx::test(migrations = "./migrations")]
async fn certification_contract_dedupes_per_attempt(pool: PgPool) {
    let svc = bare_service(&pool);
    let _badge = insert_badge(&pool, "surveyed-pro", true, Some("bronze"), "nobody", "[]", "[]", false, None).await;
    let user = Uuid::new_v4();

    let first = svc
        .on_certification_passed("cert-9", "attempt-1", user, "surveyed-pro", None)
        .await
        .expect("certification grant");
    assert!(first.created);
    assert_eq!(first.grant_key, "event:certification:cert-9:attempt-1");

    // The same certification fact redelivered: converges, no second row.
    let replay = svc
        .on_certification_passed("cert-9", "attempt-1", user, "surveyed-pro", None)
        .await
        .expect("replay");
    assert!(!replay.created);
    assert_eq!(first.grant_row_id, replay.grant_row_id);
    assert_eq!(grant_count(&pool).await, 1);

    // A new attempt is a NEW fact — one more row.
    let retake = svc
        .on_certification_passed("cert-9", "attempt-2", user, "surveyed-pro", None)
        .await
        .expect("retake");
    assert!(retake.created);
    assert_eq!(grant_count(&pool).await, 2);

    // No challenge coupling: the grant row's challenge_id stays NULL, so
    // deleting challenges can never stop a certification grant.
    let challenge_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT challenge_id FROM engagement.gamification_badge_users WHERE id = $1",
    )
    .bind(first.grant_row_id)
    .fetch_one(&pool)
    .await
    .expect("row");
    assert_eq!(challenge_id, None);

    // An unregistered badge key is a typed refusal (the M-4 contract's
    // failure shape), not a silent drop.
    let err = svc
        .on_certification_passed("cert-9", "attempt-3", user, "no-such-badge", None)
        .await
        .expect_err("unknown badge key");
    assert!(matches!(err, GamificationError::BadgeKeyUnknown(_)));
    assert_eq!(err.http_status(), 422);
}

// ─── goal definitions: the metric-registry gate ───────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn goal_definitions_require_registered_metrics(pool: PgPool) {
    let (svc, _values) = metric_service(&pool);

    let ok = svc
        .create_goal_definition(
            "Weekly sales",
            Some("sum of sales"),
            Some("EUR"),
            true,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("registered key passes");
    assert!(!ok.is_nil(), "the definition id returns");

    let err = svc
        .create_goal_definition(
            "Ghost",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("nope.metric"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect_err("unknown key refused at write");
    assert!(matches!(err, GamificationError::MetricKeyUnknown(_)));
    assert_eq!(err.http_status(), 422);

    // A computed goal with NO metric key is equally malformed.
    let err = svc
        .create_goal_definition(
            "Keyless",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            None,
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect_err("missing key refused");
    assert!(matches!(err, GamificationError::MetricKeyUnknown(_)));

    // Manual definitions need no metric and stay legal.
    svc.create_goal_definition(
        "Checklist",
        None,
        None,
        false,
        GoalComputationMode::Manual,
        None,
        GoalCondition::Lower,
        GoalDisplayMode::Boolean,
    )
    .await
    .expect("manual definition");
}

// ─── challenges: start materializes declarative membership ────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn start_materializes_membership_and_generates_goals(pool: PgPool) {
    let (svc, _values) = metric_service(&pool);
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());
    let carol = Uuid::new_v4();

    // carol holds the members badge → the include_badge_ids expansion.
    let members_badge = insert_badge(&pool, "member", true, None, "everyone", "[]", "[]", false, None).await;
    sqlx::query(
        "INSERT INTO engagement.gamification_badge_users (badge_id, recipient_user_id, grant_kind) VALUES ($1, $2, 'event')",
    )
    .bind(members_badge)
    .bind(carol)
    .execute(&pool)
    .await
    .expect("seed carol hold");

    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("definition");

    let challenge = ChallengeFixture::named("Q3 push")
        .users(&[alice, bob])
        .span(today(), Some(today() + Duration::days(30)))
        .insert(&pool)
        .await;
    sqlx::query("UPDATE engagement.gamification_challenges SET include_badge_ids = $1::jsonb WHERE id = $2")
        .bind(format!("[\"{members_badge}\"]"))
        .bind(challenge)
        .execute(&pool)
        .await
        .expect("badge filter");
    insert_line(&pool, challenge, def, 5).await;

    svc.start_challenge(challenge).await.expect("start");

    // Two explicit members + one badge holder = three live memberships.
    let members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM engagement.gamification_challenge_memberships WHERE challenge_id = $1 AND (metadata->>'deleted_at') IS NULL",
    )
    .bind(challenge)
    .fetch_one(&pool)
    .await
    .expect("memberships");
    assert_eq!(members, 3);

    // Goals generate for every member x line at current = 0, state draft.
    let goals: Vec<(String, rust_decimal::Decimal, rust_decimal::Decimal)> = sqlx::query_as(
        "SELECT state::text, target, current FROM engagement.gamification_goals WHERE challenge_id = $1",
    )
    .bind(challenge)
    .fetch_all(&pool)
    .await
    .expect("goals");
    assert_eq!(goals.len(), 3);
    assert!(goals.iter().all(|(s, _, c)| s == "draft" && c == &Decimal::ZERO));

    assert_eq!(challenge_state(&pool, challenge).await, "inprogress");
}

#[sqlx::test(migrations = "./migrations")]
async fn include_all_users_requires_a_directory(pool: PgPool) {
    let svc = bare_service(&pool); // no directory registered
    let mut fixture = ChallengeFixture::named("everyone");
    fixture.include_all = true;
    let challenge = fixture.insert(&pool).await;

    let err = svc.start_challenge(challenge).await.expect_err("no directory");
    assert!(matches!(err, GamificationError::DirectoryNotRegistered));
    assert_eq!(err.http_status(), 500);
}

// ─── the daily check: start / evaluate / realtime reward / idempotency ───────

#[sqlx::test(migrations = "./migrations")]
async fn daily_check_runs_the_full_lifecycle_idempotently(pool: PgPool) {
    let (svc, values) = metric_service(&pool);
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());

    let reward = insert_badge(&pool, "closer", true, Some("gold"), "nobody", "[]", "[]", false, None).await;
    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("definition");

    // Open-ended challenge starting yesterday: the check must start it,
    // evaluate its goals, and pay the realtime reward — in ONE run.
    let challenge = ChallengeFixture::named("Evergreen")
        .users(&[alice, bob])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    sqlx::query("UPDATE engagement.gamification_challenges SET reward_badge_id = $1 WHERE id = $2")
        .bind(reward)
        .bind(challenge)
        .execute(&pool)
        .await
        .expect("reward badge");
    insert_line(&pool, challenge, def, 2).await;

    // alice clears the target; bob does not.
    values.lock().unwrap().insert(alice, 3);
    values.lock().unwrap().insert(bob, 1);

    let report = svc.daily_challenge_check(today()).await.expect("first check");
    assert_eq!(report.challenges_started, 1);
    assert_eq!(report.goals_evaluated, 2, "both members' goals evaluate");
    assert_eq!(report.rewards_granted, 1, "only alice's all-reached pays");
    assert_eq!(challenge_state(&pool, challenge).await, "inprogress");
    let mut states = goal_states(&pool, challenge).await;
    states.sort();
    assert_eq!(states, vec!["inprogress", "reached"]);

    // The replay: nothing double-pays, nothing regenerates.
    let report = svc.daily_challenge_check(today()).await.expect("second check");
    assert_eq!(report.challenges_started, 0);
    assert_eq!(report.goals_generated, 0);
    assert_eq!(report.rewards_granted, 0);
    assert_eq!(report.rewards_deduped, 1, "alice's reward converges on the key");
    assert_eq!(grant_count(&pool).await, 1);

    // The reward row carries the challenge coupling and the key grain.
    let (kind, key, challenge_id): (String, String, Option<Uuid>) = sqlx::query_as(
        "SELECT grant_kind::text, grant_key, challenge_id FROM engagement.gamification_badge_users",
    )
    .fetch_one(&pool)
    .await
    .expect("reward row");
    assert_eq!(kind, "challenge");
    assert_eq!(key, format!("challenge:{challenge}:{reward}:{alice}"));
    assert_eq!(challenge_id, Some(challenge));
}

#[sqlx::test(migrations = "./migrations")]
async fn daily_check_closes_expired_challenges_with_podium(pool: PgPool) {
    let (svc, values) = metric_service(&pool);
    let (gold, silver, bronze) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let first = insert_badge(&pool, "first", true, None, "nobody", "[]", "[]", false, None).await;
    let second = insert_badge(&pool, "second", true, None, "nobody", "[]", "[]", false, None).await;
    let third = insert_badge(&pool, "third", true, None, "nobody", "[]", "[]", false, None).await;
    let _ = (gold, silver, bronze);

    let (u1, u2, u3) = (Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4());
    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("definition");

    // Ends TOMORROW: one run starts + evaluates; then the end date is
    // pulled into the past and the next run must close + pay the podium.
    let challenge = ChallengeFixture::named("Sprint")
        .users(&[u1, u2, u3])
        .span(today() - Duration::days(1), Some(today() + Duration::days(1)))
        .insert(&pool)
        .await;
    sqlx::query(
        r#"UPDATE engagement.gamification_challenges
               SET reward_badge_id = NULL,
                   reward_first_badge_id = $1, reward_second_badge_id = $2,
                   reward_third_badge_id = $3, reward_failure = true
               WHERE id = $4"#,
    )
    .bind(first)
    .bind(second)
    .bind(third)
    .bind(challenge)
    .execute(&pool)
    .await
    .expect("podium badges");
    insert_line(&pool, challenge, def, 2).await;

    values.lock().unwrap().insert(u1, 2);
    values.lock().unwrap().insert(u2, 2);
    values.lock().unwrap().insert(u3, 1);

    svc.daily_challenge_check(today()).await.expect("start + evaluate run");
    let mut states = goal_states(&pool, challenge).await;
    states.sort();
    assert_eq!(states, vec!["inprogress", "reached", "reached"]);

    // Time passes: the next check runs TWO days later — past the
    // challenge's end — so it must close, fail the unmet window, and pay
    // the podium. (Goal windows froze at generation; advancing the check's
    // own clock is the honest simulation.)
    let later = today() + Duration::days(2);
    let report = svc.daily_challenge_check(later).await.expect("closing run");
    assert_eq!(report.challenges_closed, 1);
    assert_eq!(report.goals_failed, 1, "the unmet inprogress goal fails its window");
    assert_eq!(challenge_state(&pool, challenge).await, "done");

    // Podium: u1/u2 fully reached (tie → uuid order), u3 admitted by the
    // reward_failure toggle. Three podium grants, all dedup-keyed.
    assert_eq!(grant_count(&pool).await, 3);
    let keys: Vec<String> = sqlx::query_scalar(
        "SELECT grant_key FROM engagement.gamification_badge_users ORDER BY grant_key",
    )
    .fetch_all(&pool)
    .await
    .expect("keys");
    assert!(keys.iter().all(|k| k.starts_with(&format!("challenge:{challenge}:"))));

    // Replaying the closing run grants nothing new.
    let report = svc.daily_challenge_check(later).await.expect("replay");
    assert_eq!(report.challenges_closed, 0);
    assert_eq!(grant_count(&pool).await, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn leaver_sweep_cancels_goals_never_unlinks(pool: PgPool) {
    let (svc, values) = metric_service(&pool);
    let (alice, bob) = (Uuid::new_v4(), Uuid::new_v4());
    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("definition");

    let challenge = ChallengeFixture::named("Open")
        .users(&[alice, bob])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    insert_line(&pool, challenge, def, 10).await;
    values.lock().unwrap().insert(alice, 1);

    svc.daily_challenge_check(today()).await.expect("start + evaluate");
    assert_eq!(goal_states(&pool, challenge).await, vec!["inprogress", "inprogress"]);

    // bob leaves the declarative set: the next reconciliation retires the
    // membership and CANCELS the goal — the row stays, challenge intact.
    sqlx::query("UPDATE engagement.gamification_challenges SET user_ids = $1::jsonb WHERE id = $2")
        .bind(format!("[\"{alice}\"]"))
        .bind(challenge)
        .execute(&pool)
        .await
        .expect("remove bob");

    let report = svc.daily_challenge_check(today()).await.expect("sweep run");
    assert_eq!(report.goals_canceled, 1);

    let bob_goal: (String, Option<Uuid>) = sqlx::query_as(
        "SELECT state::text, challenge_id FROM engagement.gamification_goals WHERE user_id = $1",
    )
    .bind(bob)
    .fetch_one(&pool)
    .await
    .expect("bob's goal");
    assert_eq!(bob_goal.0, "canceled");
    assert_eq!(bob_goal.1, Some(challenge), "canceled goals keep their challenge link");

    let live_members: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM engagement.gamification_challenge_memberships WHERE challenge_id = $1 AND (metadata->>'deleted_at') IS NULL",
    )
    .bind(challenge)
    .fetch_one(&pool)
    .await
    .expect("members");
    assert_eq!(live_members, 1);

    // alice is untouched.
    let alice_state: String = sqlx::query_scalar(
        "SELECT state::text FROM engagement.gamification_goals WHERE user_id = $1",
    )
    .bind(alice)
    .fetch_one(&pool)
    .await
    .expect("alice goal");
    assert_eq!(alice_state, "inprogress");
}

// ─── goal verbs: manual values, validated reach, reset refusal ────────────────

#[sqlx::test(migrations = "./migrations")]
async fn manual_goals_take_human_values_and_reach(pool: PgPool) {
    let (svc, _values) = metric_service(&pool);
    let alice = Uuid::new_v4();

    let def = svc
        .create_goal_definition(
            "Checklist",
            None,
            None,
            false,
            GoalComputationMode::Manual,
            None,
            GoalCondition::Lower,
            GoalDisplayMode::Boolean,
        )
        .await
        .expect("manual def");

    let challenge = ChallengeFixture::named("Manual")
        .users(&[alice])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    insert_line(&pool, challenge, def, 3).await; // lower-is-better target
    svc.start_challenge(challenge).await.expect("start");

    let goal: Uuid = sqlx::query_scalar(
        "SELECT id FROM engagement.gamification_goals WHERE user_id = $1",
    )
    .bind(alice)
    .fetch_one(&pool)
    .await
    .expect("goal");

    // The first manual write activates the draft.
    let state = svc.set_goal_current(goal, Decimal::from(5)).await.expect("set 5");
    assert_eq!(state, GoalState::Inprogress);
    // Meeting a `lower` target lands reached.
    let state = svc.set_goal_current(goal, Decimal::from(2)).await.expect("set 2");
    assert_eq!(state, GoalState::Reached);

    // A computed goal refuses human values.
    let metric_def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("metric def");
    let challenge2 = ChallengeFixture::named("Computed")
        .users(&[alice])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    insert_line(&pool, challenge2, metric_def, 5).await;
    svc.start_challenge(challenge2).await.expect("start 2");
    let goal2: Uuid = sqlx::query_scalar(
        "SELECT g.id FROM engagement.gamification_goals g WHERE g.challenge_id = $1",
    )
    .bind(challenge2)
    .fetch_one(&pool)
    .await
    .expect("goal 2");
    let err = svc.set_goal_current(goal2, Decimal::from(5)).await.expect_err("manual write refused");
    assert!(matches!(err, GamificationError::GoalNotManual));
}

#[sqlx::test(migrations = "./migrations")]
async fn manual_reach_validates_against_a_fresh_compute(pool: PgPool) {
    let (svc, values) = metric_service(&pool);
    let alice = Uuid::new_v4();

    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("def");

    let challenge = ChallengeFixture::named("Validate")
        .users(&[alice])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    insert_line(&pool, challenge, def, 5).await;

    values.lock().unwrap().insert(alice, 1); // well under target
    svc.daily_challenge_check(today()).await.expect("start + evaluate");

    let goal: Uuid = sqlx::query_scalar(
        "SELECT id FROM engagement.gamification_goals WHERE user_id = $1",
    )
    .bind(alice)
    .fetch_one(&pool)
    .await
    .expect("goal");

    // The self-service reach re-computes the metric FIRST: an unmet
    // condition is a loud refusal, never a silent revert later.
    let err = svc.manual_reach(goal).await.expect_err("unmet refusal");
    assert!(matches!(err, GamificationError::GoalCriteriaUnmet));
    assert_eq!(err.http_status(), 422);

    // Once the metric genuinely clears the target, the reach sticks.
    values.lock().unwrap().insert(alice, 6);
    svc.manual_reach(goal).await.expect("validated reach");
    let state: String = sqlx::query_scalar(
        "SELECT state::text FROM engagement.gamification_goals WHERE id = $1",
    )
    .bind(goal)
    .fetch_one(&pool)
    .await
    .expect("state");
    assert_eq!(state, "reached");
}

#[sqlx::test(migrations = "./migrations")]
async fn challenge_reset_refused_while_goals_live(pool: PgPool) {
    let (svc, values) = metric_service(&pool);
    let alice = Uuid::new_v4();
    let def = svc
        .create_goal_definition(
            "Sales",
            None,
            None,
            false,
            GoalComputationMode::RegisteredMetric,
            Some("test.sales"),
            GoalCondition::Higher,
            GoalDisplayMode::Progress,
        )
        .await
        .expect("def");

    let challenge = ChallengeFixture::named("Live")
        .users(&[alice])
        .span(today() - Duration::days(1), None)
        .insert(&pool)
        .await;
    insert_line(&pool, challenge, def, 100).await;
    values.lock().unwrap().insert(alice, 1);

    svc.daily_challenge_check(today()).await.expect("start");
    assert_eq!(goal_states(&pool, challenge).await, vec!["inprogress"]);

    // Force the done state (the sweep would normally close it); the reset
    // verb still refuses while a live goal is inprogress.
    sqlx::query("UPDATE engagement.gamification_challenges SET state = 'done' WHERE id = $1")
        .bind(challenge)
        .execute(&pool)
        .await
        .expect("force done");
    let err = svc.reset_challenge(challenge).await.expect_err("reset refused");
    assert!(
        matches!(err, GamificationError::ChallengeHasLiveGoals),
        "got: {err:?}"
    );
    assert_eq!(err.http_status(), 409);

    // A closed challenge with no live goal resets cleanly.
    sqlx::query("UPDATE engagement.gamification_goals SET state = 'failed' WHERE challenge_id = $1")
        .bind(challenge)
        .execute(&pool)
        .await
        .expect("fail goals");
    svc.reset_challenge(challenge).await.expect("reset after failure");
    assert_eq!(challenge_state(&pool, challenge).await, "draft");
}

#[sqlx::test(migrations = "./migrations")]
async fn report_cadence_fires_on_due_clock(pool: PgPool) {
    let (svc, _values) = metric_service(&pool);

    // A closed challenge with an overdue report clock: the sweep emits the
    // due payload and advances the clock.
    let challenge = ChallengeFixture::named("Reported")
        .span(today() - Duration::days(10), Some(today() - Duration::days(5)))
        .insert(&pool)
        .await;
    sqlx::query(
        "UPDATE engagement.gamification_challenges SET state = 'done', report_frequency = 'daily', next_report_date = $1 WHERE id = $2",
    )
    .bind(today() - Duration::days(1))
    .bind(challenge)
    .execute(&pool)
    .await
    .expect("seed done + due");

    let report = svc.daily_challenge_check(today()).await.expect("report run");
    assert_eq!(report.reports_due.len(), 1);
    assert_eq!(report.reports_due[0].challenge_id, challenge);
    assert!(report.reports_due[0].summary.get("members").is_some());

    let (last, next): (NaiveDate, NaiveDate) = sqlx::query_as(
        "SELECT last_report_date, next_report_date FROM engagement.gamification_challenges WHERE id = $1",
    )
    .bind(challenge)
    .fetch_one(&pool)
    .await
    .expect("clock");
    assert_eq!(last, today());
    assert_eq!(next, today() + Duration::days(1));

    // The same day's replay is not due again.
    let report = svc.daily_challenge_check(today()).await.expect("same-day replay");
    assert_eq!(report.reports_due.len(), 0);
}
