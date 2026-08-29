-- Down: drop engagement.gamification_badges table
DROP TABLE IF EXISTS engagement.gamification_badges CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_badges_audit_timestamp() CASCADE;
