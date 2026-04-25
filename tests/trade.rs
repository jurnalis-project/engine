// tests/trade.rs
//
// Black-box integration tests for merchant trade mechanics. These drive the
// engine through its public API (`new_game`, `process_input`). Coverage:
//   * Buy command: purchasing items from merchant NPCs
//   * Sell command: selling inventory items to merchant NPCs
//   * Edge cases: no merchant present, insufficient gold, item not found

use std::collections::{HashMap, HashSet};

use jurnalis_engine::{
    character::{class::Class, create_character, race::Race},
    process_input,
    state::{
        GameState, GamePhase, ProgressState, SAVE_VERSION, WorldState,
        Location, LocationType, LightLevel, Npc, NpcRole, Disposition,
        Item, ItemType, DamageType, WeaponCategory,
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

/// Build a synthetic exploration-phase GameState with a merchant NPC and
/// starting gold for trade testing.
fn make_trade_state() -> GameState {
    let mut character = create_character(
        "Trader".to_string(),
        Race::Human,
        Class::Fighter,
        scores_balanced(),
        vec![Skill::Persuasion],
    );
    // Give the character starting gold (50 gp = 5000 cp)
    character.gold_cp = 5000;

    // Create a location with a merchant
    let mut locations = HashMap::new();
    locations.insert(0, Location {
        id: 0,
        name: "Market Square".to_string(),
        description: "A bustling market square.".to_string(),
        location_type: LocationType::Room,
        exits: HashMap::new(),
        npcs: vec![0],
        items: vec![],
        triggers: vec![],
        light_level: LightLevel::Bright,
        room_features: vec![],
    });

    // Create a merchant NPC
    let mut npcs = HashMap::new();
    npcs.insert(0, Npc {
        id: 0,
        name: "Marcus the Merchant".to_string(),
        role: NpcRole::Merchant,
        disposition: Disposition::Friendly,
        dialogue_tags: vec!["trade".to_string(), "wares".to_string()],
        location: 0,
        combat_stats: None,
        conditions: Vec::new(),
    });

    GameState {
        version: SAVE_VERSION.to_string(),
        character,
        current_location: 0,
        discovered_locations: HashSet::new(),
        world: WorldState {
            locations,
            npcs,
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

// ---------- buy tests --------------------------------------------------------

#[test]
fn test_buy_command_succeeds_with_merchant_and_gold() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "buy dagger");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    // Should succeed
    let text = output.text.join(" ");
    assert!(text.contains("sells you"), "Expected purchase confirmation, got: {}", text);
    assert!(text.contains("Dagger"), "Expected item name in response, got: {}", text);

    // Gold should be deducted (Dagger costs 200 cp = 2 gp)
    assert!(new_state.character.gold_cp < 5000, "Gold should be deducted");
    assert_eq!(new_state.character.gold_cp, 4800, "Dagger costs 2 gp (200 cp)");

    // Item should be in inventory
    assert_eq!(new_state.character.inventory.len(), 1);
}

#[test]
fn test_buy_command_fails_without_merchant() {
    let mut state = make_trade_state();
    // Remove the merchant from the room
    state.world.locations.get_mut(&0).unwrap().npcs.clear();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "buy dagger");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("no merchant"), "Expected no merchant error, got: {}", text);
}

#[test]
fn test_buy_command_fails_with_insufficient_gold() {
    let mut state = make_trade_state();
    // Give character only 1 cp
    state.character.gold_cp = 1;
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "buy longsword");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("only have"), "Expected insufficient gold error, got: {}", text);
}

#[test]
fn test_buy_command_fails_for_unknown_item() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "buy unicorn horn");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("don't have"), "Expected item not found error, got: {}", text);
}

#[test]
fn test_buy_command_fuzzy_matches_items() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    // "chain" should match "Chain Mail"
    let output = process_input(&state_json, "buy chain");
    let text = output.text.join(" ");
    assert!(text.contains("Chain"), "Expected chain mail purchase, got: {}", text);
}

// ---------- sell tests -------------------------------------------------------

#[test]
fn test_sell_command_succeeds_with_inventory_item() {
    let mut state = make_trade_state();
    // Add an item to inventory
    let item_id = 100;
    state.world.items.insert(item_id, Item {
        id: item_id,
        name: "Longsword".to_string(),
        description: "A longsword.".to_string(),
        item_type: ItemType::Weapon {
            damage_dice: 1,
            damage_die: 8,
            damage_type: DamageType::Slashing,
            properties: 0,
            category: WeaponCategory::Martial,
            versatile_die: 10,
            range_normal: 0,
            range_long: 0,
        },
        location: None,
        carried_by_player: true,
        charges_remaining: None,
    });
    state.character.inventory.push(item_id);
    state.character.gold_cp = 0; // Start with no gold

    let state_json = serde_json::to_string(&state).unwrap();
    let output = process_input(&state_json, "sell longsword");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("buys your"), "Expected sale confirmation, got: {}", text);

    // Gold should be added (Longsword costs 1500 cp, sell at 50% = 750 cp)
    assert_eq!(new_state.character.gold_cp, 750, "Longsword sells for 7.5 gp (750 cp)");

    // Item should be removed from inventory
    assert!(new_state.character.inventory.is_empty());
}

#[test]
fn test_sell_command_fails_without_merchant() {
    let mut state = make_trade_state();
    // Remove the merchant
    state.world.locations.get_mut(&0).unwrap().npcs.clear();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "sell dagger");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("no merchant"), "Expected no merchant error, got: {}", text);
}

#[test]
fn test_sell_command_fails_for_nonexistent_item() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "sell dagger");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("don't have"), "Expected item not found error, got: {}", text);
}

#[test]
fn test_sell_command_fails_for_equipped_item() {
    let mut state = make_trade_state();
    // Add and equip a weapon
    let item_id = 100;
    state.world.items.insert(item_id, Item {
        id: item_id,
        name: "Longsword".to_string(),
        description: "A longsword.".to_string(),
        item_type: ItemType::Weapon {
            damage_dice: 1,
            damage_die: 8,
            damage_type: DamageType::Slashing,
            properties: 0,
            category: WeaponCategory::Martial,
            versatile_die: 10,
            range_normal: 0,
            range_long: 0,
        },
        location: None,
        carried_by_player: true,
        charges_remaining: None,
    });
    state.character.inventory.push(item_id);
    state.character.equipped.main_hand = Some(item_id);

    let state_json = serde_json::to_string(&state).unwrap();
    let output = process_input(&state_json, "sell longsword");
    let text = output.text.join(" ");
    assert!(text.to_lowercase().contains("unequip"), "Expected unequip required error, got: {}", text);
}

// ---------- merchant dialogue test -------------------------------------------

#[test]
fn test_merchant_dialogue_includes_trade_hint() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "talk marcus");
    let text = output.text.join(" ");
    assert!(text.contains("buy") || text.contains("sell"),
        "Expected trade hint in merchant dialogue, got: {}", text);
}

#[test]
fn test_merchant_dialogue_mentions_browse() {
    let state = make_trade_state();
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "talk marcus");
    let text = output.text.join(" ");
    assert!(text.contains("browse"),
        "Expected 'browse' in merchant trade hint, got: {}", text);
}

// ---------- starting gold test -----------------------------------------------

#[test]
fn test_background_starting_gold_amounts() {
    // Test that background starting gold method returns expected amounts
    use jurnalis_engine::character::background::Background;

    // Noble has the highest starting gold
    assert_eq!(Background::Noble.starting_gold_cp(), 10000, "Noble: 100 gp");
    // Merchant has high starting gold
    assert_eq!(Background::Merchant.starting_gold_cp(), 7500, "Merchant: 75 gp");
    // Most backgrounds have modest starting gold
    assert_eq!(Background::Acolyte.starting_gold_cp(), 1500, "Acolyte: 15 gp");
    // All backgrounds return > 0
    for &bg in Background::all() {
        assert!(bg.starting_gold_cp() > 0, "{:?} should have starting gold", bg);
    }
}
