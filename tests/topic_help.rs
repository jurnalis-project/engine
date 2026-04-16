use std::collections::HashMap;

use jurnalis_engine::{
    combat::{CombatState, Combatant},
    new_game, process_input,
    state::{GamePhase, GameState},
};

fn create_exploration_state_json() -> String {
    let mut output = new_game(7, false);

    // Race, Class, Background, OriginFeat, Background ability pattern, Ability method,
    // Assign scores, Choose skills, Alignment, Name
    for input in ["1", "Fighter", "1", "default", "2", "1", "15 14 13 12 10 8", "1 2", "5", "Aria"] {
        output = process_input(&output.state_json, input);
    }

    let state: GameState = serde_json::from_str(&output.state_json).unwrap();
    assert!(matches!(state.game_phase, GamePhase::Exploration));

    output.state_json
}

fn create_combat_state_json() -> String {
    let exploration_state_json = create_exploration_state_json();
    let mut state: GameState = serde_json::from_str(&exploration_state_json).unwrap();

    state.active_combat = Some(CombatState {
        initiative_order: vec![(Combatant::Player, 15)],
        current_turn: 0,
        round: 1,
        distances: HashMap::new(),
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
    });

    serde_json::to_string(&state).unwrap()
}

#[test]
fn overview_help_uses_topic_summary() {
    let state_json = create_exploration_state_json();

    let output = process_input(&state_json, "help");

    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Commands overview (exploration)")),
        "Expected exploration overview help. Got: {:?}",
        output.text
    );
    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Type 'help <topic>'")),
        "Expected topic hint in overview. Got: {:?}",
        output.text
    );
}

#[test]
fn topic_help_in_exploration_returns_focused_guidance() {
    let state_json = create_exploration_state_json();

    let output = process_input(&state_json, "help movement");

    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Help: movement (exploration)")),
        "Expected exploration movement help. Got: {:?}",
        output.text
    );
    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("go <direction>")),
        "Expected movement command details. Got: {:?}",
        output.text
    );
}

#[test]
fn topic_help_in_combat_uses_combat_context() {
    let state_json = create_combat_state_json();

    let output = process_input(&state_json, "help movement");

    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Help: movement (combat)")),
        "Expected combat movement help. Got: {:?}",
        output.text
    );
    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("approach <target>")),
        "Expected combat movement guidance. Got: {:?}",
        output.text
    );
}

#[test]
fn unknown_help_topic_lists_phase_valid_topics() {
    let state_json = create_combat_state_json();

    let output = process_input(&state_json, "help mystery");

    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Unknown help topic")),
        "Expected unknown-topic fallback. Got: {:?}",
        output.text
    );
    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("Valid topics during combat")),
        "Expected phase-aware topic list. Got: {:?}",
        output.text
    );
    assert!(
        output
            .text
            .iter()
            .any(|line| line.contains("movement, inventory, equipment, spells, system, combat")),
        "Expected combat topic set in fallback. Got: {:?}",
        output.text
    );
}
