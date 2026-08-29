-- Down: drop engagement.gamification_goal_definitions table
DROP TABLE IF EXISTS engagement.gamification_goal_definitions CASCADE;
DROP FUNCTION IF EXISTS engagement.gamification_goal_definitions_audit_timestamp() CASCADE;
