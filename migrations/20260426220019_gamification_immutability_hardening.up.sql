-- Hand-written DB hardening for the gamification + rating substrates.
-- user_owned: migrations/*gamification*immutability* (metaphor.codegen.yaml)
--
-- Installs the constraints the schema DSL cannot express:
--   1. The karma-ledger APPEND-ONLY trigger (BEFORE UPDATE OR DELETE raises —
--      soft-delete writes included; the balance projection reads the latest
--      row, so history must never change).
--   2. The rating scale CHECK (1..10, NULL while unconsumed).
--   3. The karma-rank threshold CHECK (karma_min > 0 — the generic
--      non-negative attribute would admit 0).
--
-- These back the write-path rules R-R1 / R-G2 / R-G3 declared in
-- schema/hooks/engagement.hook.yaml (enforcement: both / db / db).

-- 1. Karma ledger immutability -------------------------------------------------
-- The ONLY legal write is INSERT (the typed append verb). UPDATE and DELETE
-- raise with a stable error token; BEFORE-row firing means no row ever
-- changes, and the metadata soft-delete path (an UPDATE) is covered too.

CREATE OR REPLACE FUNCTION engagement.gamification_karma_ledger_immutable() RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION 'karma_ledger_immutable: gamification_karma_trackings is append-only (% refused)', TG_OP
        USING ERRCODE = 'P0001';
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS gamification_karma_trackings_immutable ON engagement.gamification_karma_trackings;
CREATE TRIGGER gamification_karma_trackings_immutable
    BEFORE UPDATE OR DELETE ON engagement.gamification_karma_trackings
    FOR EACH ROW EXECUTE FUNCTION engagement.gamification_karma_ledger_immutable();

-- 2. Rating scale (1..10; NULL legal while the request is unconsumed) ----------
ALTER TABLE engagement.engagement_ratings
    DROP CONSTRAINT IF EXISTS engagement_ratings_rating_value_range;
ALTER TABLE engagement.engagement_ratings
    ADD CONSTRAINT engagement_ratings_rating_value_range
    CHECK (rating_value IS NULL OR (rating_value >= 1 AND rating_value <= 10));

-- 3. Rank threshold positivity -------------------------------------------------
ALTER TABLE engagement.gamification_karma_ranks
    DROP CONSTRAINT IF EXISTS gamification_karma_ranks_karma_min_positive;
ALTER TABLE engagement.gamification_karma_ranks
    ADD CONSTRAINT gamification_karma_ranks_karma_min_positive
    CHECK (karma_min > 0);
