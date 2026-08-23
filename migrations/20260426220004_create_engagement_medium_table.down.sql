-- Down: drop engagement.engagement_media table
DROP TABLE IF EXISTS engagement.engagement_media CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_media_audit_timestamp() CASCADE;
