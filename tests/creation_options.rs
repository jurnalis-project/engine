// tests/creation_options.rs
//
// Integration tests for the `creation_options` public API. Exercises every
// `CreationField` variant through the engine's public interface.

use jurnalis_engine::{
    creation_options, new_game, process_input, CreationField, CreationOption,
};

// ---------- helpers ----------------------------------------------------------

/// Create a new game and return the serialized state.
fn fresh_state() -> String {
    let output = new_game(42, false);
    output.state_json
}

/// Drive the engine through creation steps until we reach the target step.
/// Returns the serialized state at that point.
fn state_at_step(inputs: &[&str]) -> String {
    let mut state = fresh_state();
    for input in inputs {
        let output = process_input(&state, input);
        state = output.state_json;
    }
    state
}

// ---------- Race -------------------------------------------------------------

#[test]
fn test_creation_options_race_returns_all_races() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::Race);
    // Should return 9 races matching Race::all()
    assert_eq!(options.len(), 9);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Human");
    assert_eq!(options[1].id, "2");
    assert_eq!(options[1].label, "Elf");
    assert_eq!(options[8].id, "9");
    assert_eq!(options[8].label, "Tiefling");
}

// ---------- Subrace ----------------------------------------------------------

#[test]
fn test_creation_options_subrace_empty_for_human() {
    // Choose Human (no subraces)
    let state = state_at_step(&["1"]);
    let options = creation_options(&state, CreationField::Subrace);
    assert!(options.is_empty());
}

#[test]
fn test_creation_options_subrace_for_elf() {
    // Choose Elf (has subraces)
    let state = state_at_step(&["2"]);
    let options = creation_options(&state, CreationField::Subrace);
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Drow");
    assert_eq!(options[1].id, "2");
    assert_eq!(options[1].label, "High Elf");
    assert_eq!(options[2].id, "3");
    assert_eq!(options[2].label, "Wood Elf");
}

// ---------- Class ------------------------------------------------------------

#[test]
fn test_creation_options_class_returns_all_classes() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::Class);
    assert_eq!(options.len(), 12);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Barbarian");
    assert_eq!(options[11].id, "12");
    assert_eq!(options[11].label, "Wizard");
}

// ---------- Background -------------------------------------------------------

#[test]
fn test_creation_options_background_returns_all() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::Background);
    assert_eq!(options.len(), 16);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Acolyte");
    assert_eq!(options[15].id, "16");
    assert_eq!(options[15].label, "Wayfarer");
}

// ---------- OriginFeat -------------------------------------------------------

#[test]
fn test_creation_options_origin_feat_returns_ten() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::OriginFeat);
    assert_eq!(options.len(), 10);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Alert");
    assert_eq!(options[8].id, "9");
    assert_eq!(options[8].label, "Tavern Brawler");
    assert_eq!(options[9].id, "10");
    assert_eq!(options[9].label, "Tough");
}

// ---------- BackgroundAbilityPattern -----------------------------------------

#[test]
fn test_creation_options_ability_pattern_returns_two() {
    // Need to have a background set. Drive through: race -> class -> background
    let state = state_at_step(&["1", "1", "1"]); // Human, Barbarian, Acolyte
    let options = creation_options(&state, CreationField::BackgroundAbilityPattern);
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].id, "1");
    assert!(options[0].label.contains("+2"));
    assert_eq!(options[1].id, "2");
    assert!(options[1].label.contains("+1/+1/+1"));
}

// ---------- AbilityMethod ----------------------------------------------------

#[test]
fn test_creation_options_ability_method_returns_three() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::AbilityMethod);
    assert_eq!(options.len(), 3);
    assert_eq!(options[0].id, "1");
    assert!(options[0].label.contains("Standard Array"));
    assert_eq!(options[1].id, "2");
    assert!(options[1].label.contains("Random"));
    assert_eq!(options[2].id, "3");
    assert!(options[2].label.contains("Point Buy"));
}

// ---------- Skills -----------------------------------------------------------

#[test]
fn test_creation_options_skills_for_fighter() {
    // Drive to a state where Fighter is chosen
    let state = state_at_step(&["1", "5"]); // Human, Fighter
    let options = creation_options(&state, CreationField::Skills);
    // Fighter has 8 skill choices + 1 meta entry = 9
    let meta = options.last().unwrap();
    assert_eq!(meta.id, "_meta");
    assert!(meta.label.contains("required_count:2"));
    // Actual skill options (excluding meta)
    let skills: Vec<&CreationOption> = options.iter().filter(|o| o.id != "_meta").collect();
    assert_eq!(skills.len(), 8);
    assert_eq!(skills[0].label, "Acrobatics");
}

#[test]
fn test_creation_options_skills_for_rogue() {
    let state = state_at_step(&["1", "9"]); // Human, Rogue
    let options = creation_options(&state, CreationField::Skills);
    let meta = options.last().unwrap();
    assert_eq!(meta.id, "_meta");
    assert!(meta.label.contains("required_count:4"));
    let skills: Vec<&CreationOption> = options.iter().filter(|o| o.id != "_meta").collect();
    assert_eq!(skills.len(), 11); // Rogue has 11 skill choices
}

// ---------- Alignment --------------------------------------------------------

#[test]
fn test_creation_options_alignment_returns_ten() {
    let state = fresh_state();
    let options = creation_options(&state, CreationField::Alignment);
    assert_eq!(options.len(), 10);
    assert_eq!(options[0].id, "1");
    assert_eq!(options[0].label, "Lawful Good");
    assert_eq!(options[9].id, "10");
    assert_eq!(options[9].label, "Unaligned");
}

// ---------- Edge cases -------------------------------------------------------

#[test]
fn test_creation_options_invalid_json_returns_empty() {
    let options = creation_options("not valid json", CreationField::Race);
    assert!(options.is_empty());
}

#[test]
fn test_creation_options_is_read_only() {
    let state = fresh_state();
    let _options = creation_options(&state, CreationField::Race);
    // Parse state before and after to verify no mutation
    let state_after = state.clone();
    let _options2 = creation_options(&state, CreationField::Class);
    assert_eq!(state, state_after);
}
