//! Regression guard: the unguarded `all_crud_routes()` composer must NOT mount
//! generic CRUD on the machine-bearing gamification models — the challenge and
//! goal state columns (challenge_state / goal_state) move only through the
//! write service's validated verbs (start/close/reset, evaluate/reach/cancel),
//! which also carry the verbs' side effects (membership materialization, the
//! forced reward check, the live-goal reset refusal). A generic full-row PATCH
//! would bypass the declared transition set. The generator narrows these mounts
//! to the read surface on its own; this test reads `src/lib.rs` and fails the
//! build if a regen ever re-adds a generic write mount for them.

const LIB_RS: &str = include_str!("../src/lib.rs");

/// The machine-bearing models whose generic write mounts are deliberately
/// excluded from `all_crud_routes`. Each entry is the exact write-mount call
/// site (function + the service field it would be called with).
const EXCLUDED_MACHINE_OWNED_ROUTE_MOUNTS: &[&str] = &[
    "create_gamification_challenge_routes(self.gamification_challenge_service",
    "create_gamification_goal_routes(self.gamification_goal_service",
];

#[test]
fn all_crud_routes_excludes_machine_owned_models() {
    for mount in EXCLUDED_MACHINE_OWNED_ROUTE_MOUNTS {
        assert!(
            !LIB_RS.contains(mount),
            "regression: `all_crud_routes` mounts a machine-bearing model's write route ({mount}). \
             A schema regen has re-added it. The challenge/goal state columns move only through \
             the gamification write service's verbs.",
        );
    }
}
