-- Down: drop engagement.gamification_challenge_lines table
DROP TABLE IF EXISTS engagement.gamification_challenge_lines CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_challenge_lines_audit_timestamp() CASCADE;
