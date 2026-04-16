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
        death_save_successes: 0,
        death_save_failures: 0,
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
        death_save_successes: 0,
        death_save_failures: 0,
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

// ---- Non-Wizard caster integration tests ----

/// Create an exploration-phase character for any class. The skill-choice
/// input adapts to the class's `skill_choice_count`: 2 default, 3 for
/// Bard/Ranger, 4 for Rogue.
fn create_caster_state_json(class_name: &str) -> String {
    let mut output = new_game(42, false);

    let skill_input = match class_name.to_lowercase().as_str() {
        "bard" | "ranger" => "1 2 3",
        "rogue" => "1 2 3 4",
        _ => "1 2",
    };

    // Race (Human), Class, Background (1), Origin feat (default), BG pattern (2),
    // Ability method (1 = standard array), Assign scores (15 14 13 12 10 8
    // -> STR/DEX/CON/INT/WIS/CHA), Skills (1..N), Alignment (5), Name.
    for input in ["1", class_name, "1", "default", "2", "1", "15 14 13 12 10 8", skill_input, "5", "Kael"] {
        output = process_input(&output.state_json, input);
    }

    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert!(
        matches!(state.game_phase, GamePhase::Exploration),
        "Character creation did not complete for class '{}'. Phase: {:?}",
        class_name, state.game_phase,
    );
    output.state_json
}

/// Attach a hostile goblin in-melee and start combat around the current state.
fn attach_goblin_and_start_combat(state_json: &str) -> String {
    let mut state: GameState = serde_json::from_str(state_json).unwrap();

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
            (Combatant::Player, 20),
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

#[test]
fn cleric_starts_with_class_spells() {
    let state_json = create_caster_state_json("Cleric");
    let state: GameState = serde_json::from_str(&state_json).unwrap();
    // Cleric should know some Cleric spells.
    assert!(
        state.character.known_spells.iter().any(|s| s == "Sacred Flame"),
        "Cleric should know Sacred Flame. Known: {:?}",
        state.character.known_spells
    );
    assert!(state.character.known_spells.iter().any(|s| s == "Cure Wounds"));
    assert!(state.character.known_spells.iter().any(|s| s == "Guiding Bolt"));
    // Cleric full-caster L1 slots = 2.
    assert_eq!(state.character.spell_slots_max, vec![2]);
}

#[test]
fn cleric_cast_sacred_flame_in_combat_uses_wis_not_int() {
    // The DC should be computed from Wisdom, not Intelligence, for a Cleric.
    let explore_json = create_caster_state_json("Cleric");
    let mut state: GameState = serde_json::from_str(&explore_json).unwrap();
    // Force a known Wisdom score so the DC is predictable (WIS 14 -> +2, prof +2 -> DC 12).
    state.character.ability_scores.insert(Ability::Wisdom, 14);
    // And make INT terrible so a mistaken INT-based DC would be visible (INT 1).
    state.character.ability_scores.insert(Ability::Intelligence, 1);
    let state_json = serde_json::to_string(&state).unwrap();
    let combat_json = attach_goblin_and_start_combat(&state_json);

    let output = process_input(&combat_json, "cast sacred flame at goblin");
    let text = output.text.join("\n");
    assert!(
        text.contains("DC 12"),
        "Sacred Flame should use WIS DC 12 for Cleric with WIS 14. Got:\n{}",
        text
    );
    // Cantrip -- no slot consumed.
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn cleric_cast_cure_wounds_heals_self_in_exploration() {
    let state_json = create_caster_state_json("Cleric");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    // Take some damage.
    let max_hp = state.character.max_hp;
    state.character.current_hp = (max_hp - 5).max(1);
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "cast cure wounds");
    let text = output.text.join("\n");
    assert!(
        text.contains("recover") || text.contains("HP"),
        "Expected healing narration. Got:\n{}",
        text
    );

    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    // HP should have increased.
    assert!(
        new_state.character.current_hp > max_hp - 5,
        "HP should increase after Cure Wounds. Before: {}, after: {}",
        max_hp - 5, new_state.character.current_hp,
    );
    // Slot consumed.
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn cleric_cast_healing_word_heals_self_in_exploration() {
    let state_json = create_caster_state_json("Cleric");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    let max_hp = state.character.max_hp;
    state.character.current_hp = (max_hp - 4).max(1);
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "cast healing word");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert!(new_state.character.current_hp > max_hp - 4);
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn cleric_cast_cure_wounds_at_full_hp_still_consumes_slot() {
    let state_json = create_caster_state_json("Cleric");
    // Already at full HP in fresh state.
    let output = process_input(&state_json, "cast cure wounds");
    let text = output.text.join("\n");
    assert!(
        text.to_lowercase().contains("full health"),
        "Expected 'full health' narration. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn cleric_cast_guiding_bolt_in_combat_consumes_slot() {
    let explore_json = create_caster_state_json("Cleric");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast guiding bolt at goblin");
    let text = output.text.join("\n");
    assert!(
        text.contains("radiant"),
        "Expected guiding bolt narration. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn cleric_cast_bless_starts_concentration() {
    let explore_json = create_caster_state_json("Cleric");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast bless");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(
        new_state.character.class_features.concentration_spell,
        Some("Bless".to_string()),
        "Bless should start concentration.",
    );
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn bard_cast_vicious_mockery_is_cantrip_no_slot() {
    let explore_json = create_caster_state_json("Bard");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast vicious mockery at goblin");
    let text = output.text.join("\n");
    assert!(
        text.contains("venomous insult") || text.contains("psychic") || text.contains("shrugs"),
        "Expected Vicious Mockery narration. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    // Cantrip -- no slot consumed.
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn bard_cast_charm_person_consumes_slot() {
    let explore_json = create_caster_state_json("Bard");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast charm person at goblin");
    let text = output.text.join("\n");
    assert!(
        text.contains("charm") || text.contains("enchantment") || text.contains("shrug"),
        "Expected Charm Person narration. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn druid_cast_druidcraft_is_flavor_cantrip() {
    let explore_json = create_caster_state_json("Druid");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast druidcraft");
    let text = output.text.join("\n");
    assert!(
        text.contains("nature") || text.contains("bud") || text.contains("flourish"),
        "Expected Druidcraft flavor. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    // Cantrip, no slot consumed.
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn druid_cast_faerie_fire_starts_concentration() {
    let explore_json = create_caster_state_json("Druid");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast faerie fire at goblin");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(
        new_state.character.class_features.concentration_spell,
        Some("Faerie Fire".to_string()),
    );
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn warlock_cast_eldritch_blast_cantrip_no_slot() {
    let explore_json = create_caster_state_json("Warlock");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast eldritch blast at goblin");
    let text = output.text.join("\n");
    assert!(
        text.contains("eldritch") || text.contains("force"),
        "Expected Eldritch Blast narration. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    // Cantrip, no slot consumed. Warlock L1 has 1 slot.
    assert_eq!(new_state.character.spell_slots_remaining, vec![1]);
}

#[test]
fn sorcerer_cast_mage_hand_is_flavor() {
    let explore_json = create_caster_state_json("Sorcerer");

    let output = process_input(&explore_json, "cast mage hand");
    let text = output.text.join("\n");
    assert!(
        text.contains("spectral") || text.contains("hand"),
        "Expected Mage Hand flavor. Got:\n{}",
        text
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn combat_attack_cantrip_without_target_reports_error_no_slot_change() {
    let explore_json = create_caster_state_json("Cleric");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast sacred flame");
    assert!(
        output.text.iter().any(|l| l.to_lowercase().contains("at whom")
            || l.to_lowercase().contains("at what")),
        "Expected target-needed error. Got: {:?}",
        output.text,
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn combat_leveled_spell_without_target_refunds_slot() {
    let explore_json = create_caster_state_json("Cleric");
    let combat_json = attach_goblin_and_start_combat(&explore_json);

    let output = process_input(&combat_json, "cast guiding bolt");
    assert!(
        output.text.iter().any(|l| l.to_lowercase().contains("at whom")
            || l.to_lowercase().contains("at what")),
        "Expected target-needed error. Got: {:?}",
        output.text,
    );
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
    // Slot must be refunded.
    assert_eq!(new_state.character.spell_slots_remaining, vec![2]);
}

#[test]
fn exploration_combat_spell_refuses_without_consuming_slot() {
    // Sacred Flame in exploration should emit not-in-combat without
    // changing spell slots (cantrip, but the important assertion is that we
    // don't mutate state incorrectly).
    let state_json = create_caster_state_json("Cleric");
    let output = process_input(&state_json, "cast sacred flame");
    assert!(
        output.text.iter().any(|l| l.contains("only cast that spell in combat")),
        "Expected not-in-combat message. Got: {:?}",
        output.text,
    );
}

#[test]
fn exploration_leveled_combat_spell_does_not_consume_slot() {
    // Guiding Bolt is combat-only; casting it in exploration should reject
    // without consuming a level 1 slot.
    let state_json = create_caster_state_json("Cleric");
    let before: GameState = serde_json::from_str(&state_json).unwrap();
    let before_slots = before.character.spell_slots_remaining.clone();

    let output = process_input(&state_json, "cast guiding bolt");
    assert!(
        output.text.iter().any(|l| l.contains("only cast that spell in combat")),
        "Expected not-in-combat message. Got: {:?}",
        output.text,
    );
    let after: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert_eq!(
        after.character.spell_slots_remaining, before_slots,
        "Guiding Bolt in exploration must not consume a slot.",
    );
}
