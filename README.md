# backbone-engagement

The attribution floor: UTM master data (campaign / source / medium), the
short-link tracker over it, and the tracker's deduplicated click ledger —
the port of Odoo's `utm` + `link_tracker` pair onto the Backbone module
shape.

Upstream, these two models are the seam every outbound surface writes into
(mass mailing, referrals, website forms) and every analytics read hangs
off. This module is that floor and nothing above it: it knows nothing
about mailing or websites, and nothing in it is fenced to a company
(ADR-0014 posture 4 — same as upstream, where `utm.*` rows are shared
across the whole instance).

## The surface

| Model | Table | What it is |
|---|---|---|
| `EngagementCampaign` | `engagement.engagement_campaigns` | A campaign master: unique `name` (identifier) + display `title`, plus the `is_auto_campaign` marker separating cookie-minted rows from authored ones. |
| `EngagementSource` | `engagement.engagement_sources` | A source master (`utm_source` values). |
| `EngagementMedium` | `engagement.engagement_media` | A medium master (`utm_medium` values). |
| `EngagementLinkTracker` | `engagement.engagement_link_trackers` | A tracked short link: absolute http(s) target, random `[A-Za-z0-9]` short `code`, optional caller-supplied `title`, a `label`, and the attribution triple. |
| `EngagementLinkTrackerClick` | `engagement.engagement_link_tracker_clicks` | One counted click: the link, a denormalized `campaign_id` (history is not rewritten when a tracker's attribution is edited), optional ip / country, the UTC `click_day`, and a `dedup_key`. |

The campaign model deliberately carries the display-title shape Odoo
introduced when it collapsed `mass_mailing.campaign` INTO `utm.campaign`
(v19): one campaign row serves attribution and mailing orchestration, so
the later mailing module composes onto this table instead of re-shaping
it.

## Mounting

Two separate mounts, from the built module:

```rust
let engagement = Arc::new(EngagementModule::builder().with_database(pool).build()?);

// Management surface — mount behind the host's authenticated tree.
let guarded = engagement.guarded_routes();

// Public short-link redirect — mount at the site root (codes resolve as
// /r/<code>); it is BARE (no auth: the short code is the capability) and
// throttled.
let redirect = engagement.redirect_routes();
```

`guarded_routes()` composes the generated GET-only reads for the four
masters/trackers plus the validated write seams — `POST
/engagement/attribution/resolve` (find-or-create the attribution triple),
`POST /engagement/link_trackers` (mint-or-find a tracker), `GET
/engagement/link_trackers/:id/detail`, `GET
/engagement/campaigns/:id/clicks`. Generic mutation is deliberately NOT
mounted: it would bypass the validation and idempotency the write service
enforces. The generic click-row router is not mounted either — click rows
carry visitor IPs, so counting is exposed only as the aggregated reads.

`redirect_routes()` serves `GET /r/:code`: a 302 to the stored target with
`utm_campaign` / `utm_source` / `utm_medium` injected from the tracker's
masters (existing utm_* params on the target are replaced; everything
else is preserved). It is an ADR-0019 action_link-class route — the one
sanctioned class of side-effecting GET — and carries all three of the
ADR's requirements: the click mint is idempotent (dedup key), the route
group is its explicit declaration surface, and it is rate-limited at
middleware (120 requests / 60s per client) because the exists/doesn't
distinction is an enumeration shape.

## Port decisions (decided at port time — deltas from upstream)

- **No server-side `og:title` fetch.** Upstream's link tracker fetches the
  target page server-side to auto-title the link; that is an SSRF surface
  this module refuses to own. Titles arrive from the caller; link
  previews render client-side in the host webapp.
- **302, not 301.** Upstream answers the redirect permanently; a cached
  301 would freeze the target client-side, so an edited target (or edited
  attribution masters) could never propagate. The port answers 302.
- **Validation in front of the cookie ferry.** Upstream find-or-creates
  master rows from raw browser strings. The port keeps the seam but every
  value is trimmed, length-capped, and control-character-rejected before
  anything mints; cookie planting itself stays the host webapp's concern.
- **Dedup + cap on clicks.** Upstream counts every hit. The port keys
  clicks on (tracker, ip, UTC day) so replays converge — one counted
  click per visitor per tracker per day — and freezes counting (never
  redirecting) at 100 000 rows per tracker.
- **No open redirect.** Only absolute http(s) targets with a host are
  accepted, at create time AND re-validated at redirect time.
- **One `code` column.** Upstream tracks a code and a deprecated
  prefixed variant; the port collapses to the single code, which is
  upstream's own effective invariant.
- **Percent-encoded UTM injection.** Master names are free-form text; the
  redirect builder percent-encodes them as query values so a name can
  never corrupt or spoof the target's query string.

## Deliberately not ported (yet)

- **No seed data.** Upstream ships a protected `Referral` source and six
  default mediums; hosts seed what they want through the write seams.
- **`utm.stage` / `utm.tag`** — deferred until a consumer asks for them.
- **`no_external_tracking` (ICP opt-out)** — not carried: the module
  injects UTMs on every http(s) target it tracks.
- **GeoIP resolution** — the country code is an optional string on the
  click row; a host that runs GeoIP passes it in via the
  `x-visitor-country` header on the redirect.
- **Rating / gamification** (upstream `website_links_rating`) — separate
  concern, out of scope for this module by decision.

## Layout and regeneration

The schema YAML under `schema/models/` is the single source of truth; the
`src/` layer cake is generated from it. Hand-written code lives in the
files listed under `user_owned:` in `metaphor.codegen.yaml` — the
repository (`engagement_repository.rs`), the write service
(`engagement_write_service.rs`), the two route groups — plus these tests:
`tests/attribution_cases.rs`, `tests/link_tracker_cases.rs`. Everything
else regenerates with `metaphor schema generate engagement --target all
--force`; declarations inside generated files sit in `// <<< CUSTOM` /
`// END CUSTOM` markers.

## Tests

```bash
DATABASE_URL='postgres://user:pass@localhost:5432/db' cargo test
```

The hand-written cases run on per-test scratch databases (`sqlx::test`)
with the module migrations applied. The generated
`tests/integration_tests.rs` harness exercises the CRUD surface against a
LIVE server and skips gracefully when none is reachable (set
`API_BASE_URL` to point it at a running app that mounts the module).
