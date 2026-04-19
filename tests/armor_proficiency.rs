// Integration tests for SRD 5.1 Armor Training (armor proficiency) rule:
//
//   "If you wear Light, Medium, or Heavy armor and lack training with it, you
//    have Disadvantage on any D20 Test that involves Strength or Dexterity,
//    and you can't cast spells."
//   -- docs/reference/equipment.md
//
// Hypothesis: the engine stores armor categories and tracks `equipped.body`
// but never consults the wearer's class for armor training. Wizards, Sorcerers,
// and Monks have no armor proficiencies per SRD 2024, so equipping Chain Mail
// (Heavy) must set `wearing_nonproficient_armor = true`, block spells, and
// impose disadvantage on STR/DEX attacks. Fighters are proficient with all
// armor and must stay unflagged.

use jurnalis_engine::{
    character::class::Class,
    new_game, process_input,
    state::{self, GameState},
};

/// Create a character in the exploration phase with the given class, using the
/// real character-creation flow. The sequence mirrors the wizard-building
/// helper in `spell_casting.rs`.
fn create_state_for_class(class_name: &str) -> String {
    let mut output = new_game(42, false);

    // Race=1 (Human), Class=<name>
    output = process_input(&output.state_json, "1");
    output = process_input(&output.state_json, class_name);

    // Wizard needs interactive spellbook + prepared spell selection before Background.
    if class_name.to_lowercase() == "wizard" {
        output = process_input(&output.state_json, "1 2 3 4 5 6"); // pick 6 spellbook spells
        output = process_input(&output.state_json, "1 2 3 4"); // pick 4 prepared spells per current wizard creation flow
    }

    // Background=1, Origin feat=default, Background ability pattern=2,
    // Ability method=1, Scores, Skills, Alignment=5, Name.
    for input in ["1", "default", "2", "1", "15 14 13 12 10 8", "1 2", "5", "Hero"] {
        output = process_input(&output.state_json, input);
    }
    output.state_json
}

/// Inject a Chain Mail item into the character's inventory. Returns the ID.
fn give_chain_mail(state: &mut GameState) -> u32 {
    let id = state.world.items.keys().copied().max().unwrap_or(0) + 1;
    state.world.items.insert(id, state::Item {
        id,
        name: "Chain Mail".to_string(),
        description: "Heavy interlocking metal rings.".to_string(),
        item_type: state::ItemType::Armor {
            category: state::ArmorCategory::Heavy,
            base_ac: 16,
            max_dex_bonus: Some(0),
            str_requirement: 13,
            stealth_disadvantage: true,
        },
        location: None,
        carried_by_player: true,
        charges_remaining: None,
    });
    state.character.inventory.push(id);
    id
}

/// Inject a Leather item into the character's inventory. Returns the ID.
fn give_leather(state: &mut GameState) -> u32 {
    let id = state.world.items.keys().copied().max().unwrap_or(0) + 1;
    state.world.items.insert(id, state::Item {
        id,
        name: "Leather".to_string(),
        description: "Supple hardened leather.".to_string(),
        item_type: state::ItemType::Armor {
            category: state::ArmorCategory::Light,
            base_ac: 11,
            max_dex_bonus: None,
            str_requirement: 0,
            stealth_disadvantage: false,
        },
        location: None,
        carried_by_player: true,
        charges_remaining: None,
    });
    state.character.inventory.push(id);
    id
}

// ---- Class::armor_proficiencies() contract ----

#[test]
fn wizard_has_no_armor_proficiencies() {
    assert!(Class::Wizard.armor_proficiencies().is_empty());
    assert!(Class::Sorcerer.armor_proficiencies().is_empty());
    assert!(Class::Monk.armor_proficiencies().is_empty());
}

#[test]
fn fighter_is_proficient_with_all_armor_and_shields() {
    let profs = Class::Fighter.armor_proficiencies();
    assert!(profs.contains(&state::ArmorCategory::Light));
    assert!(profs.contains(&state::ArmorCategory::Medium));
    assert!(profs.contains(&state::ArmorCategory::Heavy));
    assert!(profs.contains(&state::ArmorCategory::Shield));
}

#[test]
fn cleric_is_proficient_with_light_medium_and_shields_only() {
    let profs = Class::Cleric.armor_proficiencies();
    assert!(profs.contains(&state::ArmorCategory::Light));
    assert!(profs.contains(&state::ArmorCategory::Medium));
    assert!(profs.contains(&state::ArmorCategory::Shield));
    assert!(!profs.contains(&state::ArmorCategory::Heavy));
}

#[test]
fn rogue_and_bard_and_warlock_are_light_only() {
    for class in [Class::Rogue, Class::Bard, Class::Warlock] {
        let profs = class.armor_proficiencies();
        assert_eq!(
            profs, vec![state::ArmorCategory::Light],
            "{:?} should have exactly Light armor proficiency", class,
        );
    }
}

// ---- Equipping non-proficient armor sets the flag and warns ----

#[test]
fn wizard_equipping_chain_mail_sets_nonproficient_flag() {
    let state_json = create_state_for_class("Wizard");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    give_chain_mail(&mut state);
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "equip chain mail");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    assert!(
        new_state.character.wearing_nonproficient_armor,
        "Wizard wearing Chain Mail must set wearing_nonproficient_armor = true. Got output: {:?}",
        output.text,
    );
    let joined = output.text.join(" ");
    assert!(
        joined.contains("not proficient"),
        "Expected proficiency warning. Got: {:?}", output.text,
    );
}

#[test]
fn fighter_equipping_chain_mail_keeps_flag_false() {
    let state_json = create_state_for_class("Fighter");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    // Fighters start with Chain Mail already equipped; unequip it first to
    // exercise the equip path cleanly.
    state.character.equipped.body = None;
    give_chain_mail(&mut state);
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "equip chain mail");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    assert!(
        !new_state.character.wearing_nonproficient_armor,
        "Fighter is proficient with Heavy armor; flag must stay false. Got: {:?}",
        output.text,
    );
    let joined = output.text.join(" ");
    assert!(
        !joined.contains("not proficient"),
        "Fighter should not see a proficiency warning. Got: {:?}", output.text,
    );
}

#[test]
fn bard_equipping_leather_stays_unflagged() {
    let state_json = create_state_for_class("Bard");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    state.character.equipped.body = None;
    give_leather(&mut state);
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "equip leather");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    assert!(
        !new_state.character.wearing_nonproficient_armor,
        "Bard wearing Leather is proficient; flag must stay false. Got: {:?}",
        output.text,
    );
}

#[test]
fn unequipping_nonproficient_armor_clears_flag() {
    let state_json = create_state_for_class("Wizard");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    let armor_id = give_chain_mail(&mut state);
    state.character.equipped.body = Some(armor_id);
    state.character.wearing_nonproficient_armor = true;
    let state_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&state_json, "unequip chain mail");
    let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

    assert!(
        !new_state.character.wearing_nonproficient_armor,
        "Unequipping body armor must clear the non-proficient flag. Got: {:?}",
        output.text,
    );
}

// ---- Casting is blocked while wearing non-proficient armor ----

#[test]
fn wizard_cannot_cast_while_wearing_nonproficient_armor() {
    let state_json = create_state_for_class("Wizard");
    let mut state: GameState = serde_json::from_str(&state_json).unwrap();
    let armor_id = give_chain_mail(&mut state);
    state.character.equipped.body = Some(armor_id);
    state.character.wearing_nonproficient_armor = true;
    let blocked_json = serde_json::to_string(&state).unwrap();

    let output = process_input(&blocked_json, "cast prestidigitation");
    let joined = output.text.join(" ");
    assert!(
        joined.to_lowercase().contains("can't cast"),
        "Expected cast-blocked message. Got: {:?}", output.text,
    );

    // Control: without armor, cast works.
    let mut cleared = state.clone();
    cleared.character.wearing_nonproficient_armor = false;
    cleared.character.equipped.body = None;
    let cleared_json = serde_json::to_string(&cleared).unwrap();
    let ok_output = process_input(&cleared_json, "cast prestidigitation");
    assert!(
        !ok_output.text.join(" ").to_lowercase().contains("can't cast"),
        "Control: cast should succeed without non-proficient armor. Got: {:?}",
        ok_output.text,
    );
}

// ---- Backwards-compat: legacy saves without the field load cleanly ----

#[test]
fn legacy_save_without_flag_deserializes_with_default_false() {
    let state_json = create_state_for_class("Wizard");
    let state: GameState = serde_json::from_str(&state_json).unwrap();
    let mut v: serde_json::Value = serde_json::to_value(&state.character).unwrap();
    v.as_object_mut().unwrap().remove("wearing_nonproficient_armor");
    let loaded: jurnalis_engine::character::Character = serde_json::from_value(v).unwrap();
    assert!(!loaded.wearing_nonproficient_armor);
}
