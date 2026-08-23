-- Down: drop engagement.engagement_campaigns table
DROP TABLE IF EXISTS engagement.engagement_campaigns CASCADE;
DROP FUNCTION IF EXISTS engagement.engagement_campaigns_audit_timestamp() CASCADE;
