// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Hand-written SQL (services orchestrate, repositories hold SQL).
pub mod engagement_repository;
pub mod rating_repository;
pub mod gamification_repository;
pub use engagement_repository::EngagementRepository;
pub use rating_repository::RatingRepository;
pub use gamification_repository::GamificationRepository;
