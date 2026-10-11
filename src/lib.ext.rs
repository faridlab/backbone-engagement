//! The module's extension (hand-written; ADR-0031): what the module adds beside its
//! generated services. The generated module and builder carry `ModuleExt` and
//! `ModuleBuilderExt` and dereference to them, so their fields read as `module.field`.

#[allow(unused_imports)]
use super::*;

/// State the module adds; read through the generated module's `Deref`.
pub struct ModuleExt {
    pub(crate) engagement_write_service: std::sync::Arc<
        application::service::engagement_write_service::EngagementWriteService,
    >,
    pub(crate) rating_write_service: std::sync::Arc<
        application::service::rating_write_service::RatingWriteService,
    >,
    pub(crate) gamification_write_service: std::sync::Arc<
        application::service::gamification_write_service::GamificationWriteService,
    >,
}

/// State the builder adds.
pub struct ModuleBuilderExt {
    // custom builder fields
    rating_secret: Option<Vec<u8>>,
    rated_types: Vec<(String, Option<String>)>,
    metrics: Vec<(String, application::service::gamification_write_service::MetricFn)>,
    user_directory:
        Option<application::service::gamification_write_service::UserDirectory>,
}

impl Default for ModuleBuilderExt {
    fn default() -> Self {
        Self {
            rating_secret: None,
            rated_types: Vec::new(),
            metrics: Vec::new(),
            user_directory: None,
        }
    }
}

impl ModuleBuilderExt {
    /// Build the extension's state from what the generated build made.
    #[allow(unused_variables, clippy::redundant_clone)]
    pub(crate) fn build(self, parts: &ModuleParts<'_>) -> anyhow::Result<ModuleExt> {
        let db_pool = parts.db_pool.clone();
        let engagement_write_service = std::sync::Arc::new(
            application::service::engagement_write_service::EngagementWriteService::new(
                db_pool.clone(),
            ),
        );
        let rating_write_service = {
            let secret = self.rating_secret.unwrap_or_else(|| {
                std::env::var(
                    application::service::rating_write_service::RATING_TOKEN_SECRET_ENV,
                )
                .unwrap_or_default()
                .into_bytes()
            });
            let mut registry =
                application::service::rating_write_service::RatedTypeRegistry::new();
            for (model, parent) in &self.rated_types {
                registry.register(model, parent.as_deref());
            }
            std::sync::Arc::new(
                application::service::rating_write_service::RatingWriteService::with_config(
                    db_pool.clone(),
                    &secret,
                    registry,
                ),
            )
        };
        let gamification_write_service = {
            let mut service =
                application::service::gamification_write_service::GamificationWriteService::new(
                    db_pool.clone(),
                );
            for (key, compute) in &self.metrics {
                service.register_metric(key, compute.clone());
            }
            if let Some(directory) = &self.user_directory {
                service.set_user_directory(directory.clone());
            }
            std::sync::Arc::new(service)
        };
        Ok(ModuleExt {
            engagement_write_service,
            rating_write_service,
            gamification_write_service,
        })
    }
}

impl crate::ThisModule {
    /// The validated write path (attribution find-or-create, tracker
    /// minting, the redirect resolver) — the service behind the guarded and
    /// public route groups.
    pub fn engagement_write_service(
        &self,
    ) -> &std::sync::Arc<application::service::engagement_write_service::EngagementWriteService> {
        &self.engagement_write_service
    }
    /// Guarded management composition: safe GETs + validated write seams, NO
    /// generic mutation. Mount behind the host's authenticated tree.
    pub fn guarded_routes(self: &Arc<Self>) -> Router {
        presentation::http::guarded_routes::composer(Arc::clone(self))
    }
    /// The PUBLIC short-link redirect (`GET /r/:code`) — a BARE, throttled
    /// mount (ADR-0019 action_link class). Mount at the site root so codes
    /// resolve as `/r/<code>`; never behind auth.
    pub fn redirect_routes(self: &Arc<Self>) -> Router {
        presentation::http::redirect_routes::composer().with_state(Arc::clone(self))
    }
    /// The validated rating path (issue / rotate / reset / publisher reply /
    /// stats / the single-use submit) — the service behind the public and
    /// guarded rating route groups.
    pub fn rating_write_service(
        &self,
    ) -> &std::sync::Arc<application::service::rating_write_service::RatingWriteService> {
        &self.rating_write_service
    }
    /// The PUBLIC rating submit (`POST /rate/:token/submit`) — a BARE,
    /// throttled mount (ADR-0019 action_link class: the HMAC'd capability
    /// link is the auth). Mount at the site root; never behind auth.
    pub fn rating_public_routes(self: &Arc<Self>) -> Router {
        presentation::http::rating_routes::public_composer().with_state(Arc::clone(self))
    }
    /// Guarded rating management: the validated operator seams only (no
    /// generic rating mutation anywhere). Mount behind the host's
    /// authenticated tree.
    pub fn rating_guarded_routes(self: &Arc<Self>) -> Router {
        presentation::http::rating_routes::guarded_composer(Arc::clone(self))
    }
    /// The validated gamification path (the append-only karma ledger, the
    /// two badge-grant verbs, the certification inbound contract, both
    /// state machines, and the daily challenge check).
    pub fn gamification_write_service(
        &self,
    ) -> &std::sync::Arc<
        application::service::gamification_write_service::GamificationWriteService,
    > {
        &self.gamification_write_service
    }
}

impl crate::ThisModuleBuilder {
    /// Override the rating-token HMAC secret (tests and explicit
    /// composition; the default reads
    /// [`application::service::rating_write_service::RATING_TOKEN_SECRET_ENV`]
    /// — an unset variable leaves the secret empty and every issue/verify
    /// refuses with a typed 500).
    pub fn with_rating_secret(mut self, secret: impl Into<Vec<u8>>) -> Self {
        self.rating_secret = Some(secret.into());
        self
    }
    /// Register a rated target type (composition-time code, never free
    /// text): the entity name a rating may target, plus the optional
    /// parent model label for the rollup pair.
    pub fn with_rated_type(mut self, model: &str, parent_model: Option<&str>) -> Self {
        self.rated_types
            .push((model.to_string(), parent_model.map(String::from)));
        self
    }
    /// Register a goal metric (the name goal definitions cite; the compute
    /// function is code — the only computed mode).
    pub fn with_metric(
        mut self,
        key: &str,
        compute: application::service::gamification_write_service::MetricFn,
    ) -> Self {
        self.metrics.push((key.to_string(), compute));
        self
    }
    /// Register the user directory supplying the `include_all_users`
    /// expansion (the user table lives in another module — the host wires
    /// the listing).
    pub fn with_user_directory(
        mut self,
        directory: application::service::gamification_write_service::UserDirectory,
    ) -> Self {
        self.user_directory = Some(directory);
        self
    }
}
