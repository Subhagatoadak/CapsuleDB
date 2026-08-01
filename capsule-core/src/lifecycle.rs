//! Lifecycle transition enforcement (spec §4).

use crate::types::LifecycleState;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid lifecycle transition: {from:?} -> {to:?}")]
pub struct InvalidTransition {
    pub from: LifecycleState,
    pub to: LifecycleState,
}

/// Returns `Ok(())` if moving a capsule from `from` to `to` is a legal
/// lifecycle transition, or `Err` describing the illegal move.
///
/// Rules (spec §4):
/// - Forward-only through `draft -> in_review -> approved -> published`.
/// - `draft -> revoked` is allowed (abandoning work) without passing
///   through the other states.
/// - `published` may only move to `deprecated`, `superseded`, or `revoked`.
/// - `deprecated`, `superseded`, and `revoked` are terminal.
pub fn check_transition(from: LifecycleState, to: LifecycleState) -> Result<(), InvalidTransition> {
    use LifecycleState::*;

    let allowed = match from {
        Draft => matches!(to, InReview | Revoked),
        InReview => matches!(to, Approved | Revoked),
        Approved => matches!(to, Published | Revoked),
        Published => matches!(to, Deprecated | Superseded | Revoked),
        Deprecated | Superseded | Revoked => false,
    };

    if allowed {
        Ok(())
    } else {
        Err(InvalidTransition { from, to })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use LifecycleState::*;

    #[test]
    fn forward_path_is_allowed() {
        assert!(check_transition(Draft, InReview).is_ok());
        assert!(check_transition(InReview, Approved).is_ok());
        assert!(check_transition(Approved, Published).is_ok());
    }

    #[test]
    fn draft_can_be_abandoned_directly() {
        assert!(check_transition(Draft, Revoked).is_ok());
    }

    #[test]
    fn published_cannot_revert() {
        assert!(check_transition(Published, Draft).is_err());
        assert!(check_transition(Published, InReview).is_err());
        assert!(check_transition(Published, Approved).is_err());
    }

    #[test]
    fn published_can_only_exit_to_terminal_states() {
        assert!(check_transition(Published, Deprecated).is_ok());
        assert!(check_transition(Published, Superseded).is_ok());
        assert!(check_transition(Published, Revoked).is_ok());
    }

    #[test]
    fn terminal_states_have_no_outgoing_transitions() {
        for terminal in [Deprecated, Superseded, Revoked] {
            for target in [Draft, InReview, Approved, Published, Deprecated, Superseded, Revoked] {
                if terminal == target {
                    continue;
                }
                assert!(check_transition(terminal, target).is_err());
            }
        }
    }

    #[test]
    fn states_cannot_be_skipped() {
        assert!(check_transition(Draft, Approved).is_err());
        assert!(check_transition(Draft, Published).is_err());
        assert!(check_transition(InReview, Published).is_err());
    }
}
