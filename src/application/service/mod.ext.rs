// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Hand-authored validated write paths (user-owned; see metaphor.codegen.yaml).
pub mod engagement_write_service;
pub mod rating_write_service;
pub mod gamification_write_service;
pub use engagement_write_service::{
    AttributionInput, AttributionRefs, EngagementError, EngagementWriteService, TrackerView,
};
pub use rating_write_service::{
    RatedType, RatedTypeRegistry, RatingError, RatingStats, RatingTokenView, RatingWriteService,
};
pub use gamification_write_service::{
    CertificationGrantView, CertificationPassedFact, DailyCheckReport, GamificationError,
    GamificationMetricRegistry, GamificationWriteService, KarmaAppendView, LeaderboardEntry,
    MetricFn, RankInfo, UserDirectory,
};
