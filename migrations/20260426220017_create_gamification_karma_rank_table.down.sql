-- Down: drop engagement.gamification_karma_ranks table
DROP TABLE IF EXISTS engagement.gamification_karma_ranks CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_karma_ranks_audit_timestamp() CASCADE;
