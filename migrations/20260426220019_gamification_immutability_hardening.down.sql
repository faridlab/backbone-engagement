-- Down: remove the hand-written hardening (trigger + CHECKs).

DROP TRIGGER IF EXISTS gamification_karma_trackings_immutable ON engagement.gamification_karma_trackings;
DROP FUNCTION IF EXISTS engagement.gamification_karma_ledger_immutable() CASCADE;

ALTER TABLE engagement.engagement_ratings
    DROP CONSTRAINT IF EXISTS engagement_ratings_rating_value_range;

ALTER TABLE engagement.gamification_karma_ranks
    DROP CONSTRAINT IF EXISTS gamification_karma_ranks_karma_min_positive;
