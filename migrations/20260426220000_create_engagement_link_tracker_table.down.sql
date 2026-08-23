-- Down: drop engagement.engagement_link_trackers table
DROP TABLE IF EXISTS engagement.engagement_link_trackers CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_link_trackers_audit_timestamp() CASCADE;
