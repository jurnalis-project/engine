// jurnalis-engine/src/rest/mod.rs
// Rest mechanics: short rest and long rest per SRD 5.1.
// Dependencies: types.rs, state/, character/ (types shared via state), rules/dice.
// Does NOT depend on combat/, narration/, parser/ — orchestration in lib.rs.

use rand::Rng;
use crate::state::GameState;

/// 1 in-world hour for a short rest.
pub const SHORT_REST_MINUTES: u64 = 60;
/// 8 in-world hours for a long rest.
pub const LONG_REST_MINUTES: u64 = 60 * 8;
/// SRD 5.1 rule: no benefit from more than one long rest per 24 in-world hours.
pub const LONG_REST_COOLDOWN_MINUTES: u64 = 60 * 24;

/// Handle the `short rest` command. Returns narration lines.
/// Precondition: caller has verified the character is not in combat and is in exploration phase.
pub fn handle_short_rest(state: &mut GameState, rng: &mut impl Rng) -> Vec<String> {
    let _ = (state, rng);
    vec!["[short rest: not implemented yet]".to_string()]
}

/// Handle the `long rest` command. Returns narration lines.
/// Precondition: caller has verified the character is not in combat and is in exploration phase.
pub fn handle_long_rest(state: &mut GameState, rng: &mut impl Rng) -> Vec<String> {
    let _ = (state, rng);
    vec!["[long rest: not implemented yet]".to_string()]
}
