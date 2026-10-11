// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Hand-written route groups (user-owned; survive regen): the guarded
// management composition, the public /r/<code> redirect, and the public +
// guarded rating groups.
pub mod guarded_routes;
pub mod redirect_routes;
pub mod rating_routes;
