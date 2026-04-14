// tests/rest.rs
//
// Black-box integration tests for rest mechanics. These drive the engine
// through its public API (`new_game`, `process_input`) exactly as the
// Tauri frontend does: serialize GameState as JSON, send a text command,
// read back text output and the updated serialized state.
//
// Coverage:
//   * Short rest: hit-dice spending, HP clamp, narration, time advance,
//     Second Wind refresh (Fighter), Arcane Recovery (Wizard).
//   * Long rest: HP reset, hit-dice restore, spell-slot restore,
//     exhaustion decrement, long-rest class feature reset, cooldown
//     recording + enforcement.
//   * Denials: in combat, within 24h of previous long rest, wrong phase.
//   * Edge cases: no hit dice, already full HP, min heal = 1, non-caster
//     class with no class features.

use std::collections::{HashMap, HashSet};

use jurnalis_engine::{
    character::{class::Class, create_character, race::Race},
    combat::{CombatState, Combatant},
    new_game, process_input,
    state::{GameState, GamePhase, ProgressState, SAVE_VERSION, WorldState},
    types::{Ability, Skill},
};

// ---------- helpers ----------------------------------------------------------

fn scores_balanced() -> HashMap<Ability, i32> {
    let mut m = HashMap::new();
    m.insert(Ability::Strength, 15);
    m.insert(Ability::Dexterity, 14);
    m.insert(Ability::Constitution, 14); // +2 CON
    m.insert(Ability::Intelligence, 12);
    m.insert(Ability::Wisdom, 10);
    m.insert(Ability::Charisma, 8);
    m
}

/// Build a synthetic exploration-phase GameState for the given class.
/// Using a hand-built state (rather than running the character-creation
/// flow) keeps these tests focused on rest behavior and independent of
/// changes to the creation UI.
fn make_exploration_state(class: Class) -> GameState {
    let character = create_character(
        "Rester".to_string(),
        Race::Human,
        class,
        scores_balanced(),
        vec![Skill::Perception],
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
    }
}

fn into_json(state: &GameState) -> String {
    serde_json::to_string(state).expect("serialize GameState")
}

fn from_json(s: &str) -> GameState {
    serde_json::from_str(s).expect("deserialize GameState")
}

fn fake_combat() -> CombatState {
    CombatState {
        initiative_order: vec![(Combatant::Player, 10)],
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
    }
}

// ---------- short rest -------------------------------------------------------

#[test]
fn short_rest_heals_fighter_via_hit_dice() {
    let mut state = make_exploration_state(Class::Fighter);
    // Give the fighter more hit dice so we can observe a substantial heal.
    state.character.level = 4;
    state.character.hit_dice_remaining = 4;
    state.character.max_hp = 40;
    state.character.current_hp = 1;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    let text = out.text.join("\n");
    assert!(
        text.to_lowercase().contains("short rest"),
        "expected narration mentioning short rest, got: {}",
        text
    );
    assert!(
        new_state.character.current_hp > 1,
        "HP should increase after spending hit dice, got {}",
        new_state.character.current_hp
    );
    assert!(
        new_state.character.hit_dice_remaining < 4,
        "hit dice should decrease, got {}",
        new_state.character.hit_dice_remaining
    );
    assert_eq!(new_state.in_world_minutes, 60, "short rest = 1 hour");
}

#[test]
fn short_rest_never_exceeds_max_hp() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.level = 4;
    state.character.hit_dice_remaining = 4;
    state.character.max_hp = 40;
    state.character.current_hp = 39; // only 1 HP missing

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    assert!(
        new_state.character.current_hp <= new_state.character.max_hp,
        "HP must clamp to max"
    );
    // Only the HP gap needs to be healed, so at most one hit die should be
    // consumed.
    assert!(
        new_state.character.hit_dice_remaining >= 3,
        "should spend at most one die to cover 1-HP gap, got remaining={}",
        new_state.character.hit_dice_remaining
    );
}

#[test]
fn short_rest_min_heal_is_one_per_die_even_with_negative_con() {
    // Construct a character with CON 6 (-2 modifier). On a d10 hit die a
    // roll of 1 would give -1, but SRD floors the per-die heal at 1.
    let mut state = make_exploration_state(Class::Fighter);
    let mut bad_scores = scores_balanced();
    bad_scores.insert(Ability::Constitution, 6); // -2 modifier
    state.character.ability_scores = bad_scores;
    state.character.hit_dice_remaining = 1;
    state.character.max_hp = 20;
    state.character.current_hp = 1;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    // The player MUST end up with at least 2 HP (1 + min 1 floor).
    assert!(
        new_state.character.current_hp >= 2,
        "even with -2 CON the per-die floor is 1, got HP={}",
        new_state.character.current_hp
    );
}

#[test]
fn short_rest_with_zero_hit_dice_still_advances_time_and_notifies() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.current_hp = 4;
    state.character.hit_dice_remaining = 0;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(new_state.character.current_hp, 4, "no dice => no heal");
    assert_eq!(new_state.in_world_minutes, 60, "rest time still elapses");
    let text = out.text.join("\n").to_lowercase();
    assert!(
        text.contains("no hit dice"),
        "should explain the dice are gone, got: {}",
        text
    );
}

#[test]
fn short_rest_at_full_hp_does_not_consume_dice() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.level = 4;
    state.character.hit_dice_remaining = 4;
    state.character.max_hp = 40;
    state.character.current_hp = 40;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(new_state.character.hit_dice_remaining, 4);
    assert_eq!(new_state.character.current_hp, 40);
}

#[test]
fn short_rest_restores_fighter_second_wind() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.class_features.second_wind_available = false;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    assert!(
        new_state.character.class_features.second_wind_available,
        "Second Wind should refresh on short rest"
    );
    let text = out.text.join("\n");
    assert!(
        text.contains("Second Wind"),
        "narration should mention Second Wind, got: {}",
        text
    );
}

#[test]
fn short_rest_triggers_wizard_arcane_recovery() {
    let mut state = make_exploration_state(Class::Wizard);
    // Spend the level-1 slot so recovery has something to restore.
    assert_eq!(state.character.spell_slots_remaining, vec![2]);
    state.character.spell_slots_remaining[0] = 0;
    assert!(!state.character.class_features.arcane_recovery_used_today);

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    // At level 1 the Arcane Recovery budget is ceil(1/2) = 1 slot level.
    assert_eq!(new_state.character.spell_slots_remaining[0], 1);
    assert!(
        new_state.character.class_features.arcane_recovery_used_today,
        "Arcane Recovery should be flagged as used"
    );
    let text = out.text.join("\n");
    assert!(
        text.contains("Arcane Recovery"),
        "narration should mention Arcane Recovery, got: {}",
        text
    );
}

#[test]
fn short_rest_arcane_recovery_is_once_per_day() {
    let mut state = make_exploration_state(Class::Wizard);
    state.character.spell_slots_remaining[0] = 0;
    state.character.class_features.arcane_recovery_used_today = true;

    let out = process_input(&into_json(&state), "short rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(
        new_state.character.spell_slots_remaining[0], 0,
        "second short rest before a long rest must NOT restore slots"
    );
}

#[test]
fn short_rest_on_rogue_has_no_class_feature_narration() {
    // Rogues have no short-rest class features at level 1. The rest should
    // still work (hit-dice spend, time advance) and produce no errant
    // feature-restore lines.
    let mut state = make_exploration_state(Class::Rogue);
    state.character.current_hp = 3;
    state.character.hit_dice_remaining = 1;

    let out = process_input(&into_json(&state), "short rest");
    let text = out.text.join("\n");

    assert!(!text.contains("Second Wind"));
    assert!(!text.contains("Arcane Recovery"));
    assert!(text.to_lowercase().contains("short rest"));
}

// ---------- long rest --------------------------------------------------------

#[test]
fn long_rest_restores_hp_to_max() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.current_hp = 1;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(
        new_state.character.current_hp, new_state.character.max_hp,
        "long rest restores HP to full"
    );
}

#[test]
fn long_rest_restores_half_hit_dice_rounded_down_floor_one() {
    // Level 1 char: max dice = 1, half = 0, floor => 1 regained.
    let mut state = make_exploration_state(Class::Fighter);
    state.character.level = 1;
    state.character.hit_dice_remaining = 0;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(
        new_state.character.hit_dice_remaining, 1,
        "floor of 1 die regained even at level 1"
    );
}

#[test]
fn long_rest_restores_half_hit_dice_higher_level_and_caps_at_max() {
    // Level 6 fighter, 0 hit dice remaining: half of 6 = 3 regained.
    let mut state = make_exploration_state(Class::Fighter);
    state.character.level = 6;
    state.character.hit_dice_remaining = 0;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(new_state.character.hit_dice_remaining, 3);

    // Now test the cap: start with 5/6 dice, half = 3, but cap = 6.
    let mut state2 = make_exploration_state(Class::Fighter);
    state2.character.level = 6;
    state2.character.hit_dice_remaining = 5;
    // Use time advance to bypass the 24h cooldown check for a second test run,
    // or just don't set last_long_rest_minutes — it's a fresh state.

    let out2 = process_input(&into_json(&state2), "long rest");
    let new_state2 = from_json(&out2.state_json);

    assert_eq!(
        new_state2.character.hit_dice_remaining, 6,
        "hit dice regen caps at level"
    );
}

#[test]
fn long_rest_restores_all_spell_slots() {
    let mut state = make_exploration_state(Class::Wizard);
    state.character.spell_slots_remaining[0] = 0;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(
        new_state.character.spell_slots_remaining, new_state.character.spell_slots_max,
        "all slots restored to max on long rest"
    );
}

#[test]
fn long_rest_reduces_exhaustion_by_one_and_saturates_at_zero() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.exhaustion = 3;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);
    assert_eq!(new_state.character.exhaustion, 2);

    // Fresh state at 0 exhaustion: long rest must not underflow.
    let mut state0 = make_exploration_state(Class::Fighter);
    state0.character.exhaustion = 0;
    let out0 = process_input(&into_json(&state0), "long rest");
    let new_state0 = from_json(&out0.state_json);
    assert_eq!(new_state0.character.exhaustion, 0);
}

#[test]
fn long_rest_resets_long_rest_class_features() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.class_features.action_surge_available = false;
    state.character.class_features.second_wind_available = false;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert!(
        new_state.character.class_features.action_surge_available,
        "Action Surge resets on long rest"
    );
    assert!(
        new_state.character.class_features.second_wind_available,
        "Second Wind also resets on long rest (short-rest features always refresh)"
    );
}

#[test]
fn long_rest_resets_wizard_arcane_recovery_daily_flag() {
    let mut state = make_exploration_state(Class::Wizard);
    state.character.class_features.arcane_recovery_used_today = true;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert!(!new_state.character.class_features.arcane_recovery_used_today);
}

#[test]
fn long_rest_advances_time_eight_hours_and_records_start_time() {
    let mut state = make_exploration_state(Class::Fighter);
    state.in_world_minutes = 500;

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(new_state.in_world_minutes, 500 + 8 * 60);
    assert_eq!(
        new_state.last_long_rest_minutes,
        Some(500),
        "cooldown measured from the moment the rest began"
    );
}

// ---------- denials / interruptions -----------------------------------------

#[test]
fn rest_denied_during_combat_with_no_state_change() {
    for command in ["short rest", "long rest"] {
        let mut state = make_exploration_state(Class::Fighter);
        state.character.current_hp = 1;
        state.active_combat = Some(fake_combat());

        let before = into_json(&state);
        let out = process_input(&before, command);
        let new_state = from_json(&out.state_json);

        let text = out.text.join("\n").to_lowercase();
        assert!(
            text.contains("combat"),
            "denial should mention combat, got: {}",
            text
        );
        assert_eq!(
            new_state.character.current_hp, 1,
            "HP must not change when rest is denied (command={})",
            command
        );
        assert_eq!(
            new_state.in_world_minutes, 0,
            "time must not advance when denied (command={})",
            command
        );
        assert!(
            new_state.active_combat.is_some(),
            "combat state preserved (command={})",
            command
        );
    }
}

#[test]
fn long_rest_denied_within_cooldown_leaves_state_untouched() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.current_hp = 1;
    state.character.exhaustion = 2;
    state.last_long_rest_minutes = Some(0);
    state.in_world_minutes = 60 * 5; // 5 in-world hours later
    let snapshot = state.clone();

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    let text = out.text.join("\n").to_lowercase();
    assert!(
        text.contains("rested too recently"),
        "denial should explain cooldown, got: {}",
        text
    );
    assert_eq!(new_state.character.current_hp, snapshot.character.current_hp);
    assert_eq!(new_state.character.exhaustion, snapshot.character.exhaustion);
    assert_eq!(new_state.in_world_minutes, snapshot.in_world_minutes);
    assert_eq!(
        new_state.last_long_rest_minutes,
        snapshot.last_long_rest_minutes
    );
}

#[test]
fn long_rest_allowed_after_cooldown_fully_elapses() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.current_hp = 1;
    state.last_long_rest_minutes = Some(0);
    state.in_world_minutes = 60 * 24; // exactly 24 hours later

    let out = process_input(&into_json(&state), "long rest");
    let new_state = from_json(&out.state_json);

    assert_eq!(new_state.character.current_hp, new_state.character.max_hp);
    assert_eq!(new_state.last_long_rest_minutes, Some(60 * 24));
    assert_eq!(new_state.in_world_minutes, 60 * 24 + 8 * 60);
}

#[test]
fn short_rest_in_character_creation_is_denied() {
    // A newly started game is in CharacterCreation; `short rest` should
    // not apply there. We don't care about the exact wording; we only care
    // that no character HP / time side-effects happen.
    let output = new_game(7, false);
    let state_before = from_json(&output.state_json);
    assert!(matches!(
        state_before.game_phase,
        GamePhase::CharacterCreation(_)
    ));

    let out = process_input(&output.state_json, "short rest");
    let state_after = from_json(&out.state_json);

    // Still in character creation.
    assert!(matches!(
        state_after.game_phase,
        GamePhase::CharacterCreation(_)
    ));
    assert_eq!(
        state_after.in_world_minutes, 0,
        "no rest happens outside exploration"
    );
}

// ---------- round trips / back-to-back flows --------------------------------

#[test]
fn short_then_long_rest_sequence_produces_fully_rested_character() {
    let mut state = make_exploration_state(Class::Fighter);
    state.character.level = 3;
    state.character.max_hp = 30;
    state.character.current_hp = 5;
    state.character.hit_dice_remaining = 3;
    state.character.class_features.second_wind_available = false;
    state.character.class_features.action_surge_available = false;

    // First: short rest. Some HP regained, hit dice consumed, Second Wind back.
    let out1 = process_input(&into_json(&state), "short rest");
    let mid = from_json(&out1.state_json);
    assert!(mid.character.current_hp > 5);
    assert!(mid.character.class_features.second_wind_available);
    assert!(
        !mid.character.class_features.action_surge_available,
        "Action Surge still locked after a short rest"
    );

    // Then: long rest. Full HP, all dice within cap, Action Surge back.
    let out2 = process_input(&into_json(&mid), "long rest");
    let end = from_json(&out2.state_json);
    assert_eq!(end.character.current_hp, end.character.max_hp);
    assert!(end.character.class_features.action_surge_available);
    // Half of max (3) = 1, added to whatever remained after the short rest.
    assert!(end.character.hit_dice_remaining >= mid.character.hit_dice_remaining + 1);
    assert!(end.character.hit_dice_remaining <= end.character.level);
}
