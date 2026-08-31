# The CertificationPassed badge contract

This module CONSUMES the survey certification event. The producing module
(backbone-survey) publishes a `CertificationPassed` fact when a
certification-bearing survey attempt first passes; this module mints the
badge. This note records the contract so either side can be changed
against a written agreement, not an implicit one.

## Event

`CertificationPassed` — declared once, in this module, in
`schema/hooks/index.hook.yaml` (aggregate `GamificationBadgeUser`,
`storage.store: false`: the fact itself is not persisted; the grant row is
the durable record). The producing module declares its own outbound port
(`CertificationGrantPort` with a refusing default) and emits through it;
this module deliberately holds NO Cargo edge onto the producer and the
producer holds none onto this module.

### Payload

| Field | Type | Meaning |
|---|---|---|
| `certification_ref` | string | The certification's own id, namespaced by the producer — a survey certificate carries `survey:{survey_id}`. |
| `survey_ref` | `uuid?` | The survey that issued the certification, when known. Carried for provenance; not stored on the grant row. |
| `attempt_ref` | string | The winning attempt id. The idempotency input alongside `certification_ref`. |
| `recipient_user_id` | uuid | The certified user who earns the badge. |
| `badge_key` | string | The badge's stable key — resolved to a `GamificationBadge` by name. A miss is the typed `badge_key_unknown` refusal. |

## Idempotency: grant-key derivation

The consumer derives the idempotency key from the payload:

```
grant_key = "event:certification:{certification_ref}:{attempt_ref}"
```

Exactly one grant per (certification, attempt) under at-least-once
delivery, enforced by the `grant_key` partial UNIQUE index on
`engagement.gamification_badge_users` (live rows only). A concurrent or
replayed delivery converges on the first row: the caller sees
`created: false` and the SAME `grant_row_id` — never a second row, never
an error.

Once-per-user semantics are PRODUCER-side: the producer publishes only on
the first successful completion in the attempt pool, so a later passing
retake (a distinct `attempt_ref`) is a new key and a new grant, while a
redelivery of the same attempt is not. This module does not second-guess
that rule; it answers the (certification, attempt) grain it is handed.

## Consumer surface

`GamificationWriteService`:

- `certification_passed(&CertificationPassedFact)` — the typed entry
  point. Validates the payload shape (every reference non-empty after
  trimming, bounded so the composed key cannot overflow; the typed
  `certification_payload_invalid` refusal, HTTP 422) BEFORE any database
  work, resolves the badge by key, then grants.
- `on_certification_passed(certification_ref, attempt_ref, recipient_user_id, badge_key, survey_ref)`
  — the argument-shaped alias; constructs the fact and delegates, so
  validation and key derivation live in one place.

Both map to `grant_system(kind = event)` with:

- `challenge_id` deliberately NULL — deleting challenges can never stop a
  certification grant (no hidden coupling);
- the badge's level snapshotted onto the grant row;
- NO karma ledger write — the badge grant is the observable; karma, if a
  deployment ever wants it for certifications, rides the explicit
  `certification` origin kind of the append verb, called by the host
  adapter, not this verb.

The certification grant is a keyed system grant, NOT a peer-ladder
bypass: the peer ladder (`rule_auth` everyone/users/having/nobody, the
monthly cap, the self-grant refusal) applies only to `grant_peer`.
A certification badge is normally `rule_auth = nobody` — only the event
path can grant it.

## Adapter shape (host composition)

Delivery is in-process host composition — there is no inbox table in this
module and no outbox schema owned by the producer:

1. The host service wires an adapter implementing the producer's
   `CertificationGrantPort`.
2. The adapter builds a `CertificationPassedFact` from the producer's
   outbound payload and calls `certification_passed` (or the alias) on the
   `GamificationWriteService` obtained from the composed module.
3. The producer publishes the fact post-commit of its completion
   transaction, so a grant never rides a rolled-back attempt; redelivery
   safety on this side is the grant key.

A refusal (`badge_key_unknown`, `certification_payload_invalid`) is a
typed `Err` for the adapter to record and relay — an unregistered badge
key at publication time is a configuration error on the producing side
(the badge must exist under the exact key the survey names), surfaced
loudly here, never a silent no-grant.

Known edge, recorded: badge names carry a plain index, NOT a uniqueness
constraint — two live badges sharing a name make key resolution
first-row-wins. A deployment granting certifications must keep badge
names distinct (one natural 1:1 per certification survey on the producing
side's own `certification_badge_key` column enforces the inverse
direction). If a future release wants this structurally closed, a partial
UNIQUE over live badge names is the shape — a schema-level decision, not
this contract's.

## Probes

`tests/certification_cases.rs` pins the contract: key derivation and its
grain; payload shape validation; exactly-once under sequential replay and
under eight concurrent duplicate deliveries; the peer self-grant refusal
and the append-only karma ledger (DB trigger) unaffected by the
certification path; the typed `badge_key_unknown` miss.
`tests/gamification_cases.rs` additionally covers sequential replay, the
retake-is-a-new-fact rule, and the NULL challenge coupling.
