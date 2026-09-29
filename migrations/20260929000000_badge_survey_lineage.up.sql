-- Badge survey lineage (#239): the certification grant stamps the badge
-- with the survey whose certification first granted it. Cross-module ref
-- in the house style — no SQL FK to the survey module.
ALTER TABLE engagement.gamification_badges ADD COLUMN IF NOT EXISTS survey_id UUID;

-- Name uniqueness per lineage arm: badges carrying a survey are unique
-- per (survey, name); lineage-less badges keep the global name unique.
-- Two surveys can never mint colliding badge names.
DROP INDEX IF EXISTS engagement.uq_gamification_badges_name;
CREATE UNIQUE INDEX IF NOT EXISTS uq_gamification_badges_name_global
    ON engagement.gamification_badges (name)
    WHERE survey_id IS NULL AND (metadata->>'deleted_at') IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_gamification_badges_name_per_survey
    ON engagement.gamification_badges (survey_id, name)
    WHERE survey_id IS NOT NULL AND (metadata->>'deleted_at') IS NULL;
