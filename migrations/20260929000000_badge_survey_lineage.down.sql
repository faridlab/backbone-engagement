ALTER TABLE engagement.gamification_badges DROP COLUMN IF EXISTS survey_id;
DROP INDEX IF EXISTS engagement.uq_gamification_badges_name_global;
DROP INDEX IF EXISTS engagement.uq_gamification_badges_name_per_survey;
