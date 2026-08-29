-- Down: drop engagement.gamification_goals table
DROP TABLE IF EXISTS engagement.gamification_goals CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_goals_audit_timestamp() CASCADE;
