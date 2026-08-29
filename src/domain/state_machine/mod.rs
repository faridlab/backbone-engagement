mod challenge_state_state_machine;
mod goal_state_state_machine;

/// Shared error type for all state machines in this module
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateMachineError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Invalid transition: {0}")]
    InvalidTransition(String),

    #[error("Transition '{transition}' not allowed from state '{from}'")]
    TransitionNotAllowed {
        transition: String,
        from: String,
    },

    #[error("Role '{role}' not authorized for transition '{transition}'")]
    RoleNotAuthorized {
        role: String,
        transition: String,
    },

    #[error("Guard condition failed for transition '{0}'")]
    GuardFailed(String),

    #[error("Cannot transition from final state '{0}'")]
    FinalStateReached(String),
}

pub use challenge_state_state_machine::{challenge_stateState, challenge_stateTransition, challenge_stateStateMachine};
pub use goal_state_state_machine::{goal_stateState, goal_stateTransition, goal_stateStateMachine};
