-- Down: drop engagement.gamification_karma_trackings table
DROP TABLE IF EXISTS engagement.gamification_karma_trackings CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_karma_trackings_audit_timestamp() CASCADE;
