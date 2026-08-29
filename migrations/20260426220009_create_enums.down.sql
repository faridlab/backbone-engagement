-- Down: drop enum types for engagement module
DROP TYPE IF EXISTS goal_state CASCADE;
DROP TYPE IF EXISTS goal_display_mode CASCADE;
DROP TYPE IF EXISTS goal_condition CASCADE;
DROP TYPE IF EXISTS goal_computation_mode CASCADE;
DROP TYPE IF EXISTS membership_source CASCADE;
DROP TYPE IF EXISTS report_frequency CASCADE;
DROP TYPE IF EXISTS challenge_visibility_mode CASCADE;
DROP TYPE IF EXISTS challenge_period CASCADE;
DROP TYPE IF EXISTS challenge_state CASCADE;
DROP TYPE IF EXISTS badge_grant_kind CASCADE;
DROP TYPE IF EXISTS badge_rule_auth CASCADE;
DROP TYPE IF EXISTS badge_level CASCADE;
