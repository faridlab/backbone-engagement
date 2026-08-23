-- Down: drop engagement.engagement_link_tracker_clicks table
DROP TABLE IF EXISTS engagement.engagement_link_tracker_clicks CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_link_tracker_clicks_audit_timestamp() CASCADE;
