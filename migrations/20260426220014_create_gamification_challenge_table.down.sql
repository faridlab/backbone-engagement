-- Down: drop engagement.gamification_challenges table
DROP TABLE IF EXISTS engagement.gamification_challenges CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_challenges_audit_timestamp() CASCADE;
