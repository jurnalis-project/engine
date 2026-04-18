// jurnalis-engine/src/rest/mod.rs
// Rest mechanics: short rest and long rest per SRD 5.1.
// Dependencies: types.rs, state/, character/ (types shared via state), rules/dice.
// Does NOT depend on combat/, narration/, parser/ — orchestration in lib.rs.

use rand::Rng;
use crate::state::{GameState, GamePhase};
use crate::types::Ability;
use crate::character::class::Class;
use crate::rules::dice::roll_dice;

/// 1 in-world hour for a short rest.
pub const SHORT_REST_MINUTES: u64 = 60;
/// 8 in-world hours for a long rest.
pub const LONG_REST_MINUTES: u64 = 60 * 8;
/// SRD 5.1 rule: no benefit from more than one long rest per 24 in-world hours.
pub const LONG_REST_COOLDOWN_MINUTES: u64 = 60 * 24;

/// Reason a rest was denied. The orchestrator renders these to text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestDenial {
    InCombat,
    WrongPhase,
    /// Minutes remaining until the 24-hour cooldown elapses.
    LongRestCooldown { minutes_remaining: u64 },
}

impl RestDenial {
    pub fn to_text(&self) -> String {
        match self {
            RestDenial::InCombat => "You cannot rest during combat.".to_string(),
            RestDenial::WrongPhase => "You cannot rest right now.".to_string(),
            RestDenial::LongRestCooldown { minutes_remaining } => {
                let hours = minutes_remaining / 60;
                let minutes = minutes_remaining % 60;
                if hours > 0 && minutes > 0 {
                    format!(
                        "You've rested too recently. You must wait {} hour{} and {} minute{} before another long rest.",
                        hours,
                        if hours == 1 { "" } else { "s" },
                        minutes,
                        if minutes == 1 { "" } else { "s" },
                    )
                } else if hours > 0 {
                    format!(
                        "You've rested too recently. You must wait {} hour{} before another long rest.",
                        hours,
                        if hours == 1 { "" } else { "s" },
                    )
                } else {
                    format!(
                        "You've rested too recently. You must wait {} minute{} before another long rest.",
                        minutes,
                        if minutes == 1 { "" } else { "s" },
                    )
                }
            }
        }
    }
}

/// Check whether any rest is allowed right now. Shared by short and long rest.
pub fn check_rest_allowed(state: &GameState) -> Result<(), RestDenial> {
    if state.active_combat.is_some() {
        return Err(RestDenial::InCombat);
    }
    if !matches!(state.game_phase, GamePhase::Exploration) {
        return Err(RestDenial::WrongPhase);
    }
    Ok(())
}

/// Additional check specific to long rest: at most one per 24 in-world hours.
pub fn check_long_rest_cooldown(state: &GameState) -> Result<(), RestDenial> {
    if let Some(last) = state.last_long_rest_minutes {
        let elapsed = state.in_world_minutes.saturating_sub(last);
        if elapsed < LONG_REST_COOLDOWN_MINUTES {
            return Err(RestDenial::LongRestCooldown {
                minutes_remaining: LONG_REST_COOLDOWN_MINUTES - elapsed,
            });
        }
    }
    Ok(())
}

/// Perform a short rest. Caller must have already verified `check_rest_allowed`.
/// Returns narration lines.
pub fn perform_short_rest(state: &mut GameState, rng: &mut impl Rng) -> Vec<String> {
    let mut lines = vec!["You take a short rest (1 hour).".to_string()];

    let con_mod = Ability::modifier(
        state.character.ability_scores.get(&Ability::Constitution).copied().unwrap_or(10),
    );
    let hit_die_sides = state.character.class.hit_die();
    let available_dice = state.character.hit_dice_remaining;
    let hp_missing = state.character.max_hp - state.character.current_hp;

    if available_dice == 0 {
        lines.push("You have no hit dice remaining to spend.".to_string());
    } else if hp_missing <= 0 {
        lines.push("You are already at full HP; no hit dice spent.".to_string());
    } else {
        // Auto-spend hit dice one at a time, stopping when we reach full HP or run out.
        let mut dice_spent: u32 = 0;
        let mut total_healed: i32 = 0;
        let mut remaining_missing = hp_missing;
        while dice_spent < available_dice && remaining_missing > 0 {
            let raw_roll = roll_dice(rng, 1, hit_die_sides)[0];
            let heal = (raw_roll + con_mod).max(1); // SRD: min 1 HP per die
            let applied = heal.min(remaining_missing);
            total_healed += applied;
            remaining_missing -= applied;
            dice_spent += 1;
        }
        state.character.current_hp =
            (state.character.current_hp + total_healed).min(state.character.max_hp);
        state.character.hit_dice_remaining -= dice_spent;
        let die_word = if dice_spent == 1 { "die" } else { "dice" };
        lines.push(format!(
            "You spend {} hit {} and recover {} HP. (HP: {}/{}, hit dice remaining: {})",
            dice_spent,
            die_word,
            total_healed,
            state.character.current_hp,
            state.character.max_hp,
            state.character.hit_dice_remaining,
        ));
    }

    // Short-rest class feature resets / triggers
    lines.extend(apply_short_rest_class_features(state));

    state.in_world_minutes += SHORT_REST_MINUTES;

    lines
}

/// Apply short-rest class-feature refreshes for supported classes.
/// Returns any narration lines describing recoveries.
fn apply_short_rest_class_features(state: &mut GameState) -> Vec<String> {
    let mut lines = Vec::new();
    match state.character.class {
        Class::Fighter => {
            if !state.character.class_features.second_wind_available {
                state.character.class_features.second_wind_available = true;
                lines.push("Your Second Wind is restored.".to_string());
            }
        }
        Class::Wizard => {
            // Arcane Recovery: once per day, during a short rest, restore
            // combined slot levels up to ceil(wizard_level / 2) from the
            // smallest available.
            if !state.character.class_features.arcane_recovery_used_today {
                let level = state.character.level.max(1);
                let budget = (level + 1) / 2; // ceil(level/2)
                let recovered = recover_slots_up_to_budget(state, budget);
                if recovered > 0 {
                    state.character.class_features.arcane_recovery_used_today = true;
                    lines.push(format!(
                        "Arcane Recovery restores {} spell slot level{}.",
                        recovered,
                        if recovered == 1 { "" } else { "s" },
                    ));
                }
            }
        }
        Class::Barbarian => {
            // Barbarian: one Rage use recovers on a short rest (capped at max).
            let level = state.character.level.max(1);
            let max_rages = barbarian_rage_max(level);
            if state.character.class_features.rage_uses_remaining < max_rages {
                state.character.class_features.rage_uses_remaining += 1;
                lines.push("A Rage use returns to you.".to_string());
            }
        }
        Class::Cleric | Class::Paladin => {
            // Channel Divinity: restore to per-level cap on short rest.
            let level = state.character.level.max(1);
            let cap = channel_divinity_max(state.character.class, level);
            if cap > 0 && state.character.class_features.channel_divinity_remaining < cap {
                state.character.class_features.channel_divinity_remaining = cap;
                lines.push("Channel Divinity is restored.".to_string());
            }
        }
        Class::Monk => {
            // Ki refreshes fully on a short rest.
            let level = state.character.level.max(1);
            let max_ki = monk_ki_max(level);
            if max_ki > 0 && state.character.class_features.ki_points_remaining < max_ki {
                state.character.class_features.ki_points_remaining = max_ki;
                lines.push("Your ki is restored.".to_string());
            }
        }
        Class::Warlock => {
            // Warlock Pact Magic slots refresh on a short rest.
            let before: i32 = state.character.spell_slots_remaining.iter().sum();
            let max_total: i32 = state.character.spell_slots_max.iter().sum();
            if before < max_total {
                state.character.spell_slots_remaining = state.character.spell_slots_max.clone();
                lines.push("Your Pact Magic slots are restored.".to_string());
            }
        }
        Class::Rogue | Class::Bard | Class::Druid | Class::Ranger | Class::Sorcerer => {
            // No short-rest class features at MVP levels for these classes.
        }
    }
    lines
}

/// Barbarian: number of Rage uses at the given level (per SRD 2024 table).
fn barbarian_rage_max(level: u32) -> u32 {
    match level {
        0..=2 => 2,
        3..=5 => 3,
        6..=11 => 4,
        12..=16 => 5,
        _ => 6,
    }
}

/// Monk: Ki / Focus points at the given level (equal to Monk level per SRD).
fn monk_ki_max(level: u32) -> u32 {
    // Monks gain Ki / Focus at level 2 in 5e SRD.
    if level < 2 { 0 } else { level }
}

/// Channel Divinity: uses per short-rest cycle at the given level.
/// Cleric: 1 at level 2+, 2 at level 6+, 3 at level 18+.
/// Paladin: 1 at level 3+, 2 at level 11+, 3 at level 20.
fn channel_divinity_max(class: Class, level: u32) -> u32 {
    match class {
        Class::Cleric => match level {
            0..=1 => 0,
            2..=5 => 1,
            6..=17 => 2,
            _ => 3,
        },
        Class::Paladin => match level {
            0..=2 => 0,
            3..=10 => 1,
            11..=19 => 2,
            _ => 3,
        },
        _ => 0,
    }
}

/// Restore spell slots starting from the lowest level, spending up to
/// `budget` combined slot levels total. Returns the total slot levels restored.
/// (Arcane Recovery restriction: 5e RAW disallows using budget on level-6+
/// slots, but at MVP level 1 this doesn't matter.)
fn recover_slots_up_to_budget(state: &mut GameState, budget: u32) -> u32 {
    let mut spent: u32 = 0;
    let max_len = state.character.spell_slots_max.len();
    for i in 0..max_len {
        let slot_level = (i as u32) + 1;
        // Skip level 6+ per RAW (irrelevant at MVP but future-proof).
        if slot_level >= 6 {
            break;
        }
        let max_slots = state.character.spell_slots_max[i];
        while state.character.spell_slots_remaining[i] < max_slots {
            if spent + slot_level > budget {
                return spent;
            }
            state.character.spell_slots_remaining[i] += 1;
            spent += slot_level;
        }
    }
    spent
}

/// Perform a long rest. Caller must have already verified `check_rest_allowed`
/// and `check_long_rest_cooldown`. Returns narration lines.
pub fn perform_long_rest(state: &mut GameState, _rng: &mut impl Rng) -> Vec<String> {
    let mut lines = vec!["You take a long rest (8 hours).".to_string()];

    // HP -> max
    let hp_restored = state.character.max_hp - state.character.current_hp;
    state.character.current_hp = state.character.max_hp;
    if hp_restored > 0 {
        lines.push(format!("Your HP is restored to {}/{}.", state.character.current_hp, state.character.max_hp));
    }

    // Hit dice: restore max(1, max/2), capped at level (max hit dice).
    let max_hit_dice = state.character.level.max(1);
    let regained = (max_hit_dice / 2).max(1);
    let new_total = (state.character.hit_dice_remaining + regained).min(max_hit_dice);
    let actually_regained = new_total - state.character.hit_dice_remaining;
    state.character.hit_dice_remaining = new_total;
    if actually_regained > 0 {
        let die_word = if actually_regained == 1 { "die" } else { "dice" };
        lines.push(format!(
            "You regain {} hit {} ({} remaining).",
            actually_regained,
            die_word,
            state.character.hit_dice_remaining,
        ));
    }

    // Spell slots -> max
    let total_slots_before: i32 = state.character.spell_slots_remaining.iter().sum();
    let total_slots_max: i32 = state.character.spell_slots_max.iter().sum();
    if !state.character.spell_slots_max.is_empty() && total_slots_before < total_slots_max {
        state.character.spell_slots_remaining = state.character.spell_slots_max.clone();
        lines.push("All spell slots are restored.".to_string());
    }

    // Exhaustion reduce by 1 (saturating)
    if state.character.exhaustion > 0 {
        state.character.exhaustion -= 1;
        lines.push(format!(
            "Your exhaustion eases (now level {}).",
            state.character.exhaustion,
        ));
    }

    // Long-rest feature resets
    let mut feature_resets = Vec::new();
    let class = state.character.class;
    let level = state.character.level.max(1);

    if !state.character.class_features.action_surge_available
        && matches!(class, Class::Fighter)
    {
        state.character.class_features.action_surge_available = true;
        feature_resets.push("Action Surge");
    }
    if state.character.class_features.arcane_recovery_used_today
        && matches!(class, Class::Wizard)
    {
        state.character.class_features.arcane_recovery_used_today = false;
        feature_resets.push("Arcane Recovery");
    }
    // Short-rest features also refresh on a long rest.
    if !state.character.class_features.second_wind_available
        && matches!(class, Class::Fighter)
    {
        state.character.class_features.second_wind_available = true;
        feature_resets.push("Second Wind");
    }
    // Barbarian: refund all Rage uses + clear rage_active.
    if matches!(class, Class::Barbarian) {
        let max_rages = barbarian_rage_max(level);
        if state.character.class_features.rage_uses_remaining < max_rages {
            state.character.class_features.rage_uses_remaining = max_rages;
            feature_resets.push("Rage");
        }
        state.character.class_features.rage_active = false;
    }
    // Bard: refresh Bardic Inspiration (= max(1, CHA_mod)).
    if matches!(class, Class::Bard) {
        let cha_mod = Ability::modifier(
            state.character.ability_scores.get(&Ability::Charisma).copied().unwrap_or(10),
        );
        let max_ins = cha_mod.max(1) as u32;
        if state.character.class_features.bardic_inspiration_remaining < max_ins {
            state.character.class_features.bardic_inspiration_remaining = max_ins;
            feature_resets.push("Bardic Inspiration");
        }
    }
    // Cleric / Paladin: refresh Channel Divinity to level cap.
    if matches!(class, Class::Cleric | Class::Paladin) {
        let cap = channel_divinity_max(class, level);
        if cap > 0 && state.character.class_features.channel_divinity_remaining < cap {
            state.character.class_features.channel_divinity_remaining = cap;
            feature_resets.push("Channel Divinity");
        }
    }
    // Monk: refresh Ki.
    if matches!(class, Class::Monk) {
        let max_ki = monk_ki_max(level);
        if max_ki > 0 && state.character.class_features.ki_points_remaining < max_ki {
            state.character.class_features.ki_points_remaining = max_ki;
            feature_resets.push("Ki");
        }
    }
    // Turn-scoped rogue flags also clear on a long rest (defensive).
    state.character.class_features.cunning_action_used = false;
    state.character.class_features.sneak_attack_used_this_turn = false;

    if !feature_resets.is_empty() {
        lines.push(format!("Class features refreshed: {}.", feature_resets.join(", ")));
    }

    // Record the rest time at the START of the rest per spec (so cooldown is
    // measured from when we began resting). Then advance time.
    state.last_long_rest_minutes = Some(state.in_world_minutes);
    state.in_world_minutes += LONG_REST_MINUTES;

    lines.push("You feel fully rested.".to_string());
    lines
}

/// Orchestrator entry point for the `short rest` command.
pub fn handle_short_rest(state: &mut GameState, rng: &mut impl Rng) -> Vec<String> {
    if let Err(denial) = check_rest_allowed(state) {
        return vec![denial.to_text()];
    }
    perform_short_rest(state, rng)
}

/// Orchestrator entry point for the `long rest` command.
pub fn handle_long_rest(state: &mut GameState, rng: &mut impl Rng) -> Vec<String> {
    if let Err(denial) = check_rest_allowed(state) {
        return vec![denial.to_text()];
    }
    if let Err(denial) = check_long_rest_cooldown(state) {
        return vec![denial.to_text()];
    }
    perform_long_rest(state, rng)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{create_character, race::Race, class::Class as CharClass};
    use crate::state::{WorldState, GamePhase, SAVE_VERSION, ProgressState};
    use std::collections::{HashMap, HashSet};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_scores() -> HashMap<Ability, i32> {
        let mut m = HashMap::new();
        m.insert(Ability::Strength, 15);
        m.insert(Ability::Dexterity, 14);
        m.insert(Ability::Constitution, 14); // +2 CON
        m.insert(Ability::Intelligence, 12);
        m.insert(Ability::Wisdom, 10);
        m.insert(Ability::Charisma, 8);
        m
    }

    fn make_state(class: CharClass) -> GameState {
        let character = create_character(
            "Rester".to_string(),
            Race::Human,
            class,
            test_scores(),
            vec![],
        );
        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(),
                npcs: HashMap::new(),
                items: HashMap::new(),
                triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 0,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: ProgressState::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
            pending_new_game_confirm: false,
        }
    }

    // --- check_rest_allowed ---

    #[test]
    fn test_rest_denied_during_combat() {
        let mut state = make_state(CharClass::Fighter);
        // Fake a combat state by injecting an empty CombatState; we only care that
        // active_combat is Some.
        use crate::combat::CombatState;
        state.active_combat = Some(CombatState {
            initiative_order: Vec::new(),
            current_turn: 0,
            round: 1,
            distances: HashMap::new(),
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: false,
            action_used: false,
            bonus_action_used: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: crate::types::Cover::None,
            npc_cover: std::collections::HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        });
        assert_eq!(check_rest_allowed(&state), Err(RestDenial::InCombat));
    }

    #[test]
    fn test_rest_denied_in_character_creation() {
        let mut state = make_state(CharClass::Fighter);
        state.game_phase = GamePhase::CharacterCreation(crate::state::CreationStep::ChooseRace);
        assert_eq!(check_rest_allowed(&state), Err(RestDenial::WrongPhase));
    }

    #[test]
    fn test_rest_allowed_in_exploration() {
        let state = make_state(CharClass::Fighter);
        assert_eq!(check_rest_allowed(&state), Ok(()));
    }

    // --- short rest ---

    #[test]
    fn test_short_rest_heals_when_damaged() {
        let mut state = make_state(CharClass::Fighter);
        // Take some damage so hit-die spending is useful.
        state.character.current_hp = 4;
        let initial_max = state.character.max_hp;
        let initial_dice = state.character.hit_dice_remaining;

        let mut rng = StdRng::seed_from_u64(42);
        let lines = perform_short_rest(&mut state, &mut rng);

        // HP increased (at least 1, since min heal = 1)
        assert!(state.character.current_hp > 4);
        // HP clamped to max
        assert!(state.character.current_hp <= initial_max);
        // At least one hit die spent
        assert!(state.character.hit_dice_remaining < initial_dice);
        // Narration mentions hit die + HP
        assert!(lines.iter().any(|l| l.contains("hit die") || l.contains("hit dice")));
    }

    #[test]
    fn test_short_rest_with_no_hit_dice_notifies() {
        let mut state = make_state(CharClass::Fighter);
        state.character.current_hp = 4;
        state.character.hit_dice_remaining = 0;

        let mut rng = StdRng::seed_from_u64(42);
        let lines = perform_short_rest(&mut state, &mut rng);
        assert_eq!(state.character.current_hp, 4); // unchanged
        assert!(lines.iter().any(|l| l.to_lowercase().contains("no hit dice")));
    }

    #[test]
    fn test_short_rest_at_full_hp_skips_hit_dice() {
        let mut state = make_state(CharClass::Fighter);
        let initial_dice = state.character.hit_dice_remaining;
        // Already at full HP
        let mut rng = StdRng::seed_from_u64(42);
        let _ = perform_short_rest(&mut state, &mut rng);
        assert_eq!(state.character.hit_dice_remaining, initial_dice);
    }

    #[test]
    fn test_short_rest_advances_time_one_hour() {
        let mut state = make_state(CharClass::Fighter);
        let mut rng = StdRng::seed_from_u64(42);
        let before = state.in_world_minutes;
        perform_short_rest(&mut state, &mut rng);
        assert_eq!(state.in_world_minutes, before + SHORT_REST_MINUTES);
    }

    #[test]
    fn test_short_rest_restores_fighter_second_wind() {
        let mut state = make_state(CharClass::Fighter);
        state.character.class_features.second_wind_available = false;
        let mut rng = StdRng::seed_from_u64(42);
        let lines = perform_short_rest(&mut state, &mut rng);
        assert!(state.character.class_features.second_wind_available);
        assert!(lines.iter().any(|l| l.contains("Second Wind")));
    }

    #[test]
    fn test_short_rest_wizard_arcane_recovery_restores_slots() {
        let mut state = make_state(CharClass::Wizard);
        // Spend a slot so recovery has work to do
        state.character.spell_slots_remaining[0] = 0;
        assert!(!state.character.class_features.arcane_recovery_used_today);

        let mut rng = StdRng::seed_from_u64(42);
        let lines = perform_short_rest(&mut state, &mut rng);

        // At level 1, budget = ceil(1/2) = 1, so 1 first-level slot is recovered.
        assert_eq!(state.character.spell_slots_remaining[0], 1);
        assert!(state.character.class_features.arcane_recovery_used_today);
        assert!(lines.iter().any(|l| l.contains("Arcane Recovery")));
    }

    #[test]
    fn test_short_rest_arcane_recovery_blocked_after_use() {
        let mut state = make_state(CharClass::Wizard);
        state.character.spell_slots_remaining[0] = 0;
        state.character.class_features.arcane_recovery_used_today = true;

        let mut rng = StdRng::seed_from_u64(42);
        let _ = perform_short_rest(&mut state, &mut rng);
        // Slots NOT restored since Arcane Recovery was already used today
        assert_eq!(state.character.spell_slots_remaining[0], 0);
    }

    // --- long rest ---

    #[test]
    fn test_long_rest_restores_hp_to_max() {
        let mut state = make_state(CharClass::Fighter);
        state.character.current_hp = 1;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.current_hp, state.character.max_hp);
    }

    #[test]
    fn test_long_rest_restores_half_hit_dice_min_1() {
        let mut state = make_state(CharClass::Fighter);
        // Level 1 character: max dice = 1, half = 0, but floor of 1 applies.
        state.character.hit_dice_remaining = 0;
        state.character.level = 1;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.hit_dice_remaining, 1);
    }

    #[test]
    fn test_long_rest_restores_half_hit_dice_higher_level() {
        let mut state = make_state(CharClass::Fighter);
        state.character.level = 6;
        state.character.hit_dice_remaining = 0;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        // max = 6, half = 3
        assert_eq!(state.character.hit_dice_remaining, 3);
    }

    #[test]
    fn test_long_rest_caps_hit_dice_at_max() {
        let mut state = make_state(CharClass::Fighter);
        state.character.level = 4;
        state.character.hit_dice_remaining = 3; // one below max, regen would give 2 -> cap at 4
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.hit_dice_remaining, 4);
    }

    #[test]
    fn test_long_rest_restores_all_spell_slots() {
        let mut state = make_state(CharClass::Wizard);
        state.character.spell_slots_remaining[0] = 0;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.spell_slots_remaining, state.character.spell_slots_max);
    }

    #[test]
    fn test_long_rest_reduces_exhaustion() {
        let mut state = make_state(CharClass::Fighter);
        state.character.exhaustion = 3;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.exhaustion, 2);
    }

    #[test]
    fn test_long_rest_exhaustion_saturates_at_zero() {
        let mut state = make_state(CharClass::Fighter);
        state.character.exhaustion = 0;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.character.exhaustion, 0);
    }

    #[test]
    fn test_long_rest_resets_action_surge() {
        let mut state = make_state(CharClass::Fighter);
        state.character.class_features.action_surge_available = false;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert!(state.character.class_features.action_surge_available);
    }

    #[test]
    fn test_long_rest_resets_arcane_recovery_flag() {
        let mut state = make_state(CharClass::Wizard);
        state.character.class_features.arcane_recovery_used_today = true;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert!(!state.character.class_features.arcane_recovery_used_today);
    }

    #[test]
    fn test_long_rest_advances_time_eight_hours_and_records_start() {
        let mut state = make_state(CharClass::Fighter);
        state.in_world_minutes = 100;
        let mut rng = StdRng::seed_from_u64(42);
        perform_long_rest(&mut state, &mut rng);
        assert_eq!(state.in_world_minutes, 100 + LONG_REST_MINUTES);
        // last_long_rest_minutes records the time at the START of the rest.
        assert_eq!(state.last_long_rest_minutes, Some(100));
    }

    #[test]
    fn test_long_rest_cooldown_blocks_second_rest() {
        let mut state = make_state(CharClass::Fighter);
        state.last_long_rest_minutes = Some(0);
        state.in_world_minutes = 60 * 10; // only 10 in-world hours later
        let result = check_long_rest_cooldown(&state);
        match result {
            Err(RestDenial::LongRestCooldown { minutes_remaining }) => {
                assert_eq!(minutes_remaining, LONG_REST_COOLDOWN_MINUTES - 60 * 10);
            }
            other => panic!("Expected cooldown denial, got {:?}", other),
        }
    }

    #[test]
    fn test_long_rest_cooldown_allows_after_24_hours() {
        let mut state = make_state(CharClass::Fighter);
        state.last_long_rest_minutes = Some(0);
        state.in_world_minutes = LONG_REST_COOLDOWN_MINUTES;
        assert_eq!(check_long_rest_cooldown(&state), Ok(()));
    }

    #[test]
    fn test_long_rest_never_rested_allowed() {
        let state = make_state(CharClass::Fighter);
        assert_eq!(state.last_long_rest_minutes, None);
        assert_eq!(check_long_rest_cooldown(&state), Ok(()));
    }

    // --- orchestrator handlers: denial paths ---

    #[test]
    fn test_handle_short_rest_denied_in_combat_returns_message_no_state_change() {
        let mut state = make_state(CharClass::Fighter);
        use crate::combat::CombatState;
        state.active_combat = Some(CombatState {
            initiative_order: Vec::new(),
            current_turn: 0,
            round: 1,
            distances: HashMap::new(),
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: false,
            action_used: false,
            bonus_action_used: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: crate::types::Cover::None,
            npc_cover: std::collections::HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        });
        state.character.current_hp = 1;
        let before_time = state.in_world_minutes;
        let before_hp = state.character.current_hp;

        let mut rng = StdRng::seed_from_u64(42);
        let lines = handle_short_rest(&mut state, &mut rng);

        assert!(lines.iter().any(|l| l.to_lowercase().contains("combat")));
        assert_eq!(state.in_world_minutes, before_time);
        assert_eq!(state.character.current_hp, before_hp);
    }

    #[test]
    fn test_handle_long_rest_denied_by_cooldown_no_state_change() {
        let mut state = make_state(CharClass::Fighter);
        state.character.current_hp = 1;
        state.character.exhaustion = 2;
        state.in_world_minutes = 60 * 2; // 2 hours since last rest
        state.last_long_rest_minutes = Some(0);

        let before_hp = state.character.current_hp;
        let before_exh = state.character.exhaustion;
        let before_time = state.in_world_minutes;
        let before_last = state.last_long_rest_minutes;

        let mut rng = StdRng::seed_from_u64(42);
        let lines = handle_long_rest(&mut state, &mut rng);

        assert!(lines.iter().any(|l| l.to_lowercase().contains("rested too recently")));
        assert_eq!(state.character.current_hp, before_hp);
        assert_eq!(state.character.exhaustion, before_exh);
        assert_eq!(state.in_world_minutes, before_time);
        assert_eq!(state.last_long_rest_minutes, before_last);
    }

    #[test]
    fn test_rest_denial_to_text_mentions_hours_and_minutes() {
        let denial = RestDenial::LongRestCooldown { minutes_remaining: 60 * 3 + 15 };
        let text = denial.to_text();
        assert!(text.contains("3 hours"));
        assert!(text.contains("15 minutes"));
    }
}
