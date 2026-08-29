-- Down: drop engagement.engagement_ratings table
DROP TABLE IF EXISTS engagement.engagement_ratings CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_ratings_audit_timestamp() CASCADE;
