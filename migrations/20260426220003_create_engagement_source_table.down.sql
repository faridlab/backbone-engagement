-- Down: drop engagement.engagement_sources table
DROP TABLE IF EXISTS engagement.engagement_sources CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_sources_audit_timestamp() CASCADE;
