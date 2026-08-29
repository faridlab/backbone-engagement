-- Down: drop engagement.gamification_challenge_memberships table
DROP TABLE IF EXISTS engagement.gamification_challenge_memberships CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_challenge_memberships_audit_timestamp() CASCADE;
