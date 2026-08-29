-- Down: drop engagement.gamification_badge_users table
DROP TABLE IF EXISTS engagement.gamification_badge_users CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_badge_users_audit_timestamp() CASCADE;
