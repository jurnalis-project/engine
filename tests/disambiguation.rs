// tests/disambiguation.rs
//
// Integration tests for item disambiguation numeric selection (#62).
//
// Hypothesis: The disambiguation prompt (`format_disambiguation`) shows
// a numbered list like "1. Shortsword / 2. Shortbow", but the parser has
// no concept of numeric selection. When the player responds with "1",
// the parser produces `Command::Unknown("1")`. The fix routes numeric
// input through a pending-disambiguation context stored on `GameState`,
// so the orchestrator can look up the selected candidate and re-dispatch
// the original command with the exact name.
//
// These tests exercise the end-to-end flow through `process_input`.

use std::collections::{HashMap, HashSet};

use jurnalis_engine::{
    character::{class::Class, create_character, race::Race},
    new_game, process_input,
    state::{
        DamageType, GameState, GamePhase, Item, ItemType, Location, LocationType,
        LightLevel, ProgressState, SAVE_VERSION, WeaponCategory, WorldState,
    },
    types::{Ability, Skill},
};

// ---------- helpers ----------------------------------------------------------

fn scores_balanced() -> HashMap<Ability, i32> {
    let mut m = HashMap::new();
    m.insert(Ability::Strength, 15);
    m.insert(Ability::Dexterity, 14);
    m.insert(Ability::Constitution, 14);
    m.insert(Ability::Intelligence, 12);
    m.insert(Ability::Wisdom, 10);
    m.insert(Ability::Charisma, 8);
    m
}

/// Construct a minimal exploration-phase state with a single room that holds
/// two items whose names share the prefix "sh" (Shortsword, Shortbow). The
/// player is NOT carrying either item; both sit in the room.
fn make_two_item_room_state() -> GameState {
    let character = create_character(
        "Picker".to_string(),
        Race::Human,
        Class::Fighter,
        scores_balanced(),
        vec![Skill::Perception],
    );

    let shortsword = Item {
        id: 1,
        name: "Shortsword".to_string(),
        description: "A light, pointed blade.".to_string(),
        item_type: ItemType::Weapon {
            damage_dice: 1,
            damage_die: 6,
            damage_type: DamageType::Piercing,
            properties: 0,
            category: WeaponCategory::Martial,
            versatile_die: 0,
            range_normal: 0,
            range_long: 0,
        },
        location: Some(0),
        carried_by_player: false,
        charges_remaining: None,
    };
    let shortbow = Item {
        id: 2,
        name: "Shortbow".to_string(),
        description: "A compact ranged weapon.".to_string(),
        item_type: ItemType::Weapon {
            damage_dice: 1,
            damage_die: 6,
            damage_type: DamageType::Piercing,
            properties: 0,
            category: WeaponCategory::Simple,
            versatile_die: 0,
            range_normal: 80,
            range_long: 320,
        },
        location: Some(0),
        carried_by_player: false,
        charges_remaining: None,
    };

    let room = Location {
        id: 0,
        name: "Dusty Armory".to_string(),
        description: "Racks of old weapons line the walls.".to_string(),
        location_type: LocationType::Room,
        exits: HashMap::new(),
        npcs: Vec::new(),
        items: vec![1, 2],
        triggers: Vec::new(),
        light_level: LightLevel::Bright,
    };

    let mut items = HashMap::new();
    items.insert(1, shortsword);
    items.insert(2, shortbow);

    let mut locations = HashMap::new();
    locations.insert(0, room);

    GameState {
        version: SAVE_VERSION.to_string(),
        character,
        current_location: 0,
        discovered_locations: HashSet::from([0]),
        world: WorldState {
            locations,
            npcs: HashMap::new(),
            items,
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
        pending_disambiguation: None,
    }
}

/// Same as `make_two_item_room_state`, but the two items are already in the
/// player's inventory. Used for `drop` / `equip` disambiguation tests.
fn make_two_item_inventory_state() -> GameState {
    let mut s = make_two_item_room_state();
    for id in [1u32, 2u32] {
        if let Some(item) = s.world.items.get_mut(&id) {
            item.location = None;
            item.carried_by_player = true;
        }
    }
    s.world.locations.get_mut(&0).unwrap().items.clear();
    s.character.inventory = vec![1, 2];
    s
}

fn into_json(state: &GameState) -> String {
    serde_json::to_string(state).expect("serialize GameState")
}

fn from_json(s: &str) -> GameState {
    serde_json::from_str(s).expect("deserialize GameState")
}

// ---------- regression: numeric selection resolves disambiguation -----------

/// Core bug fix: after a `take sh` prompt shows a numbered list, the player
/// can type "1" to pick the first candidate and "2" to pick the second.
#[test]
fn numeric_selection_resolves_take_disambiguation() {
    let state = make_two_item_room_state();
    let out = process_input(&into_json(&state), "take sh");
    let prompt_text = out.text.join("\n");
    assert!(
        prompt_text.contains("Which do you mean?"),
        "initial ambiguous take should emit disambiguation prompt, got: {}",
        prompt_text,
    );
    assert!(
        prompt_text.contains("Shortsword") && prompt_text.contains("Shortbow"),
        "prompt should list both candidates, got: {}",
        prompt_text,
    );

    // Second turn: player types "1" expecting to take the first listed item.
    let out2 = process_input(&out.state_json, "1");
    let reply = out2.text.join("\n");
    assert!(
        !reply.contains("Which do you mean?"),
        "numeric selection should NOT re-prompt, got: {}",
        reply,
    );
    // Success narration is the templated pickup line ("You pick up the X."
    // or "You take the X.")
    let lower = reply.to_lowercase();
    assert!(
        lower.contains("pick up") || lower.contains("take"),
        "expected take/pickup narration after numeric selection, got: {}",
        reply,
    );

    // Verify state: exactly one of the two items now carried by player.
    let new_state = from_json(&out2.state_json);
    let carried: Vec<u32> = new_state
        .world
        .items
        .values()
        .filter(|i| i.carried_by_player)
        .map(|i| i.id)
        .collect();
    assert_eq!(carried.len(), 1, "exactly one item should be taken, got {:?}", carried);
}

/// The second candidate in the list (index 2) resolves correctly.
#[test]
fn numeric_selection_second_candidate() {
    let state = make_two_item_room_state();
    let out = process_input(&into_json(&state), "take sh");
    let prompt = out.text.join("\n");
    // Determine the order the prompt lists candidates in, so the assertion
    // can be made regardless of HashMap iteration order.
    let shortsword_first =
        prompt.find("Shortsword").unwrap() < prompt.find("Shortbow").unwrap();

    let out2 = process_input(&out.state_json, "2");
    let new_state = from_json(&out2.state_json);
    let carried_name = new_state
        .world
        .items
        .values()
        .find(|i| i.carried_by_player)
        .map(|i| i.name.clone())
        .expect("one item should have been picked up");
    let expected = if shortsword_first { "Shortbow" } else { "Shortsword" };
    assert_eq!(carried_name, expected, "entering '2' should pick the second listed candidate");
}

/// Non-numeric input after a disambiguation prompt clears the pending state
/// and is processed as a fresh command.
#[test]
fn non_numeric_input_clears_pending_disambiguation() {
    let state = make_two_item_room_state();
    let out = process_input(&into_json(&state), "take sh");
    assert!(out.text.join("\n").contains("Which do you mean?"));

    // Player types something unrelated. It should be processed normally and
    // the pending state must be cleared (so a subsequent "1" must NOT resolve
    // to the earlier list).
    let out2 = process_input(&out.state_json, "look");
    let look_text = out2.text.join("\n");
    // `look` in an otherwise-empty room should render the room name, not the
    // disambiguation prompt.
    assert!(
        !look_text.contains("Which do you mean?"),
        "non-numeric input should not re-emit the prompt, got: {}",
        look_text,
    );

    // Now "1" alone is no longer a valid selection and must NOT pick an item.
    let out3 = process_input(&out2.state_json, "1");
    let new_state = from_json(&out3.state_json);
    let any_carried = new_state.world.items.values().any(|i| i.carried_by_player);
    assert!(
        !any_carried,
        "after a non-numeric input, '1' must not resolve a stale list"
    );
}

/// Out-of-range numeric input (e.g., "5" when only 2 candidates) is treated
/// as unknown input; it must not panic, must not resolve, and must clear the
/// pending state so the player can recover.
#[test]
fn out_of_range_numeric_input_does_not_resolve() {
    let state = make_two_item_room_state();
    let out = process_input(&into_json(&state), "take sh");
    assert!(out.text.join("\n").contains("Which do you mean?"));

    let out2 = process_input(&out.state_json, "5");
    let new_state = from_json(&out2.state_json);
    let any_carried = new_state.world.items.values().any(|i| i.carried_by_player);
    assert!(
        !any_carried,
        "an out-of-range number must not resolve any candidate"
    );
}

/// Disambiguation works for `equip` as well. Regression: scope lists equip
/// among the affected commands.
#[test]
fn numeric_selection_resolves_equip_disambiguation() {
    let state = make_two_item_inventory_state();
    let out = process_input(&into_json(&state), "equip sh");
    let prompt = out.text.join("\n");
    assert!(
        prompt.contains("Which do you mean?"),
        "ambiguous equip should emit disambiguation prompt, got: {}",
        prompt,
    );

    let out2 = process_input(&out.state_json, "1");
    let reply = out2.text.join("\n");
    assert!(
        !reply.contains("Which do you mean?"),
        "numeric selection after equip prompt should resolve, got: {}",
        reply,
    );
    let new_state = from_json(&out2.state_json);
    assert!(
        new_state.character.equipped.main_hand.is_some(),
        "main hand should be filled after resolving equip disambiguation",
    );
}

/// A fresh game state has no pending disambiguation, so a bare "1" from the
/// player during exploration must NOT cause any side effects.
#[test]
fn bare_numeric_without_pending_prompt_is_harmless() {
    // Drive the engine through `new_game` so we don't depend on any
    // internal creation-phase details. A bare "1" during the race-selection
    // step is a valid creation input, so we skip that by building an
    // exploration state manually.
    let state = make_two_item_room_state();
    let out = process_input(&into_json(&state), "1");
    let new_state = from_json(&out.state_json);
    let any_carried = new_state.world.items.values().any(|i| i.carried_by_player);
    assert!(!any_carried, "bare '1' with no pending prompt must not take anything");
}

/// new_game should still work and the ChooseRace step's numeric selection
/// (which is a separate code path) must continue to function. This guards
/// against accidentally short-circuiting creation-phase numeric input.
#[test]
fn character_creation_numeric_selection_still_works() {
    let out = new_game(7, false);
    let out2 = process_input(&out.state_json, "1");
    let new_state = from_json(&out2.state_json);
    // After picking race 1 (Human), phase advances to ChooseClass.
    match new_state.game_phase {
        GamePhase::CharacterCreation(step) => {
            use jurnalis_engine::state::CreationStep;
            assert_eq!(step, CreationStep::ChooseClass, "race selection should advance to class step");
        }
        other => panic!("expected CharacterCreation phase, got {:?}", other),
    }
}
