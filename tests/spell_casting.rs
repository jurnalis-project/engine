use std::collections::HashMap;

use jurnalis_engine::{
    combat::{CombatState, Combatant},
    new_game, process_input,
    state::{GameState, GamePhase, CombatStats, NpcAttack, DamageType, Npc, NpcRole, Disposition},
    types::{Ability, NpcId},
};

/// Create an exploration-phase Wizard character.
fn create_wizard_state_json() -> String {
    let mut output = new_game(42, false);

    // Race, Class, Background, Origin feat, Background ability pattern,
    // Ability method, Assign scores, Choose skills, Alignment, Name
    for input in ["1", "Wizard", "1", "default", "2", "1", "15 14 13 12 10 8", "1 2", "5", "Elara"] {
        output = process_input(&output.state_json, input);
    }

    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert!(matches!(state.game_phase, GamePhase::Exploration));
    assert_eq!(state.character.class, jurnalis_engine::character::class::Class::Wizard);

    output.state_json
}

/// Create a Fighter (non-caster) in exploration.
fn create_fighter_state_json() -> String {
    let mut output = new_game(42, false);

    for input in ["1", "Fighter", "1", "default", "2", "1", "15 14 13 12 10 8", "1 2", "5", "Conan"] {
        output = process_input(&output.state_json, input);
    }

    output.state_json
}

/// Put a Wizard into combat against a goblin.
fn create_wizard_combat_state_json() -> String {
    let wizard_json = create_wizard_state_json();
    let mut state: GameState = serde_json::from_str(&wizard_json).unwrap();

    let npc_id: NpcId = 9000;
    let goblin = Npc {
        id: npc_id,
        name: "Goblin".to_string(),
        role: NpcRole::Guard,
        disposition: Disposition::Hostile,
        dialogue_tags: vec![],
        location: state.current_location,
        combat_stats: Some(CombatStats {
            max_hp: 7,
            current_hp: 7,
            ac: 13,
            speed: 30,
            ability_scores: {
                let mut s = HashMap::new();
                s.insert(Ability::Strength, 8);
                s.insert(Ability::Dexterity, 14);
                s.insert(Ability::Constitution, 10);
                s.insert(Ability::Intelligence, 10);
                s.insert(Ability::Wisdom, 8);
                s.insert(Ability::Charisma, 8);
                s
            },
            attacks: vec![NpcAttack {
                name: "Scimitar".to_string(),
                hit_bonus: 4,
                damage_dice: 1,
                damage_die: 6,
                damage_bonus: 2,
                damage_type: DamageType::Slashing,
                reach: 5,
                range_normal: 0,
                range_long: 0,
            }],
            proficiency_bonus: 2,
            cr: 0.25,
            ..Default::default()
        }),
        conditions: vec![],
    };
    state.world.npcs.insert(npc_id, goblin);

    let mut distances = HashMap::new();
    distances.insert(npc_id, 5);

    state.active_combat = Some(CombatState {
        initiative_order: vec![
            (Combatant::Player, 15),
            (Combatant::Npc(npc_id), 10),
        ],
        current_turn: 0,
        round: 1,
        distances,
        player_movement_remaining: state.character.speed,
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
    });

    serde_json::to_string(&state).unwrap()
}

/// Put a Wizard into combat with multiple goblins for AoE testing.
fn create_wizard_multi_combat_state_json() -> String {
    let wizard_json = create_wizard_state_json();
    let mut state: GameState = serde_json::from_str(&wizard_json).unwrap();

    let make_goblin = |id: NpcId, name: &str, hp: i32| -> Npc {
        Npc {
            id,
            name: name.to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(CombatStats {
                max_hp: hp,
                current_hp: hp,
                ac: 13,
                speed: 30,
                ability_scores: {
                    let mut s = HashMap::new();
                    s.insert(Ability::Dexterity, 14);
                    s
                },
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 0.25,
                ..Default::default()
            }),
            conditions: vec![],
        }
    };

    state.world.npcs.insert(9001, make_goblin(9001, "Goblin Grunt", 5));
    state.world.npcs.insert(9002, make_goblin(9002, "Goblin Scout", 7));

    let mut distances = HashMap::new();
    distances.insert(9001, 5); // melee
    distances.insert(9002, 5); // melee

    state.active_combat = Some(CombatState {
        initiative_order: vec![
            (Combatant::Player, 15),
            (Combatant::Npc(9001), 10),
            (Combatant::Npc(9002), 8),
        ],
        current_turn: 0,
        round: 1,
        distances,
        player_movement_remaining: state.character.speed,
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
    });

    serde_json::to_string(&state).unwrap()
}

// --- Tests ---

#[test]
fn non_wizard_cannot_cast() {
    let state_json = create_fighter_state_json();
    let output = process_input(&state_json, "cast fire bolt");

    assert!(
        output.text.iter().any(|l| l.contains("don't know any spells")),
        "Expected not-a-caster message. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_unknown_spell() {
    let state_json = create_wizard_state_json();
    let output = process_input(&state_json, "cast fireball");

    assert!(
        output.text.iter().any(|l| l.contains("don't know that spell")),
        "Expected unknown spell message. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_prestidigitation_in_exploration() {
    let state_json = create_wizard_state_json();
    let output = process_input(&state_json, "cast prestidigitation");

    assert!(
        output.text.iter().any(|l| l.contains("sparks")),
        "Expected prestidigitation flavor text. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_combat_spell_outside_combat() {
    let state_json = create_wizard_state_json();
    let output = process_input(&state_json, "cast magic missile");

    assert!(
        output.text.iter().any(|l| l.contains("only cast that spell in combat")),
        "Expected not-in-combat message. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_fire_bolt_exploration_flavor() {
    let state_json = create_wizard_state_json();
    let output = process_input(&state_json, "cast fire bolt");

    // Fire bolt in exploration should give flavor text, not "only in combat"
    assert!(
        output.text.iter().any(|l| l.contains("nothing to throw it at")),
        "Expected fire bolt exploration flavor text. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_fire_bolt_in_combat() {
    let state_json = create_wizard_combat_state_json();
    let output = process_input(&state_json, "cast fire bolt at goblin");

    let text = output.text.join(" ");
    // Should contain fire bolt narration (hit or miss)
    assert!(
        text.contains("bolt of fire") || text.contains("fire"),
        "Expected fire bolt narration. Got: {:?}",
        output.text
    );
    // Fire bolt is a cantrip, no slot should be consumed
    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(state.character.spell_slots_remaining, vec![2], "Cantrip should not consume slots");
}

#[test]
fn wizard_cast_magic_missile_in_combat() {
    let state_json = create_wizard_combat_state_json();
    let output = process_input(&state_json, "cast magic missile at goblin");

    let text = output.text.join(" ");
    assert!(
        text.contains("darts of force") || text.contains("force damage"),
        "Expected magic missile narration. Got: {:?}",
        output.text
    );
    // Should consume 1 first-level slot
    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(state.character.spell_slots_remaining, vec![1], "Should have consumed a slot");
}

#[test]
fn wizard_cast_burning_hands_in_combat() {
    let state_json = create_wizard_multi_combat_state_json();
    let output = process_input(&state_json, "cast burning hands");

    let text = output.text.join(" ");
    assert!(
        text.contains("Flames") || text.contains("fire"),
        "Expected burning hands narration. Got: {:?}",
        output.text
    );
    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(state.character.spell_slots_remaining, vec![1], "Should have consumed a slot");
}

#[test]
fn wizard_cast_sleep_in_combat() {
    let state_json = create_wizard_multi_combat_state_json();
    let output = process_input(&state_json, "cast sleep");

    let text = output.text.join(" ");
    assert!(
        text.contains("drowsiness") || text.contains("sleep"),
        "Expected sleep narration. Got: {:?}",
        output.text
    );
    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(state.character.spell_slots_remaining, vec![1], "Should have consumed a slot");
}

#[test]
fn wizard_cast_shield_in_combat() {
    let state_json = create_wizard_combat_state_json();
    let output = process_input(&state_json, "cast shield");

    let text = output.text.join(" ");
    assert!(
        text.contains("barrier") || text.contains("+5 AC"),
        "Expected shield narration. Got: {:?}",
        output.text
    );
    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(state.character.spell_slots_remaining, vec![1], "Should have consumed a slot");
}

#[test]
fn wizard_no_slots_remaining() {
    let wizard_json = create_wizard_combat_state_json();
    let mut state: GameState = serde_json::from_str(&wizard_json).unwrap();
    state.character.spell_slots_remaining = vec![0]; // deplete slots
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "cast magic missile at goblin");

    assert!(
        output.text.iter().any(|l| l.contains("no spell slots remaining")),
        "Expected no-slots message. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_cast_fire_bolt_needs_target_in_combat() {
    let state_json = create_wizard_combat_state_json();
    let output = process_input(&state_json, "cast fire bolt");

    assert!(
        output.text.iter().any(|l| l.contains("at whom") || l.contains("at what")),
        "Expected target-needed message. Got: {:?}",
        output.text
    );
}

#[test]
fn wizard_spell_slots_serialize_roundtrip() {
    let state_json = create_wizard_state_json();
    let state: GameState = serde_json::from_str(&state_json).unwrap();
    assert_eq!(state.character.spell_slots_max, vec![2]);
    assert_eq!(state.character.spell_slots_remaining, vec![2]);
    assert_eq!(state.character.known_spells.len(), 6);

    // Save and load
    let json = serde_json::to_string(&state).unwrap();
    let loaded: GameState = serde_json::from_str(&json).unwrap();
    assert_eq!(loaded.character.spell_slots_max, vec![2]);
    assert_eq!(loaded.character.spell_slots_remaining, vec![2]);
    assert_eq!(loaded.character.known_spells.len(), 6);
}
