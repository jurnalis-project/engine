pub mod templates;

use crate::rules::checks::CheckResult;
use crate::state::{GameState, Location};
use rand::Rng;
use std::collections::HashMap;

pub fn narrate_enter_location(
    rng: &mut impl Rng,
    location: &Location,
    state: &GameState,
    npc_barks: &[(String, String)],
) -> Vec<String> {
    let mut lines = Vec::new();

    // Location description
    let template = templates::pick(rng, templates::ENTER_LOCATION);
    lines.push(
        template
            .replace("{name}", &location.name)
            .replace("{description}", &location.description),
    );

    // Exits
    if !location.exits.is_empty() {
        let exit_names: Vec<String> = location.exits.keys().map(|d| d.to_string()).collect();
        lines.push(templates::EXITS.replace("{exits}", &exit_names.join(", ")));
    }

    // NPCs (exclude dead NPCs — those with combat_stats where current_hp <= 0)
    if !location.npcs.is_empty() {
        let npc_names: Vec<String> = location
            .npcs
            .iter()
            .filter_map(|id| state.world.npcs.get(id))
            .filter(|npc| match &npc.combat_stats {
                Some(stats) => stats.current_hp > 0,
                None => true, // Friendly/neutral NPCs without combat_stats are always shown
            })
            .map(|npc| npc.display_name())
            .collect();
        if !npc_names.is_empty() {
            lines.push(templates::NPCS_PRESENT.replace("{npcs}", &npc_names.join(", ")));
        }
    }

    // NPC ambient barks
    for (name, bark) in npc_barks {
        lines.push(format!("{}: {}", name, bark));
    }

    // Items
    if !location.items.is_empty() {
        let item_names: Vec<String> = location
            .items
            .iter()
            .filter_map(|id| state.world.items.get(id))
            .filter(|item| !item.carried_by_player)
            .map(|item| item.name.clone())
            .collect();
        if !item_names.is_empty() {
            lines.push(templates::ITEMS_PRESENT.replace("{items}", &item_names.join(", ")));
        }
    }

    if !location.room_features.is_empty() {
        let feature_names: Vec<String> = location
            .room_features
            .iter()
            .map(|feature| feature.name.clone())
            .collect();
        lines.push(format!("Notable features: {}.", feature_names.join(", ")));
    }

    lines
}

pub fn narrate_look(
    rng: &mut impl Rng,
    location: &Location,
    state: &GameState,
    npc_barks: &[(String, String)],
) -> Vec<String> {
    let mut lines = Vec::new();

    let template = templates::pick(rng, templates::LOOK_LOCATION);
    lines.push(
        template
            .replace("{name}", &location.name)
            .replace("{description}", &location.description),
    );

    if !location.exits.is_empty() {
        let exit_names: Vec<String> = location.exits.keys().map(|d| d.to_string()).collect();
        lines.push(templates::EXITS.replace("{exits}", &exit_names.join(", ")));
    }

    // NPCs (exclude dead NPCs — those with combat_stats where current_hp <= 0)
    if !location.npcs.is_empty() {
        let npc_names: Vec<String> = location
            .npcs
            .iter()
            .filter_map(|id| state.world.npcs.get(id))
            .filter(|npc| match &npc.combat_stats {
                Some(stats) => stats.current_hp > 0,
                None => true, // Friendly/neutral NPCs without combat_stats are always shown
            })
            .map(|npc| npc.display_name())
            .collect();
        if !npc_names.is_empty() {
            lines.push(templates::NPCS_PRESENT.replace("{npcs}", &npc_names.join(", ")));
        }
    }

    // NPC ambient barks
    for (name, bark) in npc_barks {
        lines.push(format!("{}: {}", name, bark));
    }

    if !location.items.is_empty() {
        let item_names: Vec<String> = location
            .items
            .iter()
            .filter_map(|id| state.world.items.get(id))
            .filter(|item| !item.carried_by_player)
            .map(|item| item.name.clone())
            .collect();
        if !item_names.is_empty() {
            lines.push(templates::ITEMS_PRESENT.replace("{items}", &item_names.join(", ")));
        }
    }

    if !location.room_features.is_empty() {
        let feature_names: Vec<String> = location
            .room_features
            .iter()
            .map(|feature| feature.name.clone())
            .collect();
        lines.push(format!("Notable features: {}.", feature_names.join(", ")));
    }

    lines
}

pub fn narrate_skill_check(rng: &mut impl Rng, skill: &str, result: &CheckResult) -> String {
    let templates = if result.success {
        templates::SKILL_CHECK_SUCCESS
    } else {
        templates::SKILL_CHECK_FAILURE
    };
    let template = templates::pick(rng, templates);
    template
        .replace("{skill}", skill)
        .replace("{roll}", &result.roll.to_string())
        .replace("{mod}", &result.modifier.to_string())
        .replace("{total}", &result.total.to_string())
        .replace("{dc}", &result.dc.to_string())
}

fn format_weapon_slot(
    items: &HashMap<crate::types::ItemId, crate::state::Item>,
    id: crate::types::ItemId,
) -> String {
    match items.get(&id) {
        Some(item) => {
            if let crate::state::ItemType::Weapon {
                damage_dice,
                damage_die,
                damage_type,
                versatile_die,
                ..
            } = &item.item_type
            {
                if *damage_dice == 0 {
                    return item.name.clone(); // Net, etc.
                }
                let base = format!(
                    "{} ({}d{} {}",
                    item.name, damage_dice, damage_die, damage_type
                );
                if *versatile_die > 0 {
                    format!("{}, versatile 1d{})", base, versatile_die)
                } else {
                    format!("{})", base)
                }
            } else {
                item.name.clone()
            }
        }
        None => "(empty)".to_string(),
    }
}

fn format_armor_slot(
    items: &HashMap<crate::types::ItemId, crate::state::Item>,
    id: crate::types::ItemId,
) -> String {
    match items.get(&id) {
        Some(item) => {
            if let crate::state::ItemType::Armor {
                base_ac,
                stealth_disadvantage,
                ..
            } = &item.item_type
            {
                let disadv = if *stealth_disadvantage {
                    ", stealth disadvantage"
                } else {
                    ""
                };
                format!("{} (AC {}{})", item.name, base_ac, disadv)
            } else {
                item.name.clone()
            }
        }
        None => "(none)".to_string(),
    }
}

fn format_equip_slot(
    items: &HashMap<crate::types::ItemId, crate::state::Item>,
    id: crate::types::ItemId,
) -> String {
    match items.get(&id) {
        Some(item) => match &item.item_type {
            crate::state::ItemType::Armor {
                category: crate::state::ArmorCategory::Shield,
                base_ac,
                ..
            } => {
                format!("{} (+{} AC)", item.name, base_ac)
            }
            crate::state::ItemType::Weapon { .. } => format_weapon_slot(items, id),
            _ => item.name.clone(),
        },
        None => "(empty)".to_string(),
    }
}

pub fn narrate_character_sheet(state: &GameState) -> Vec<String> {
    let c = &state.character;
    let mut lines = Vec::new();
    lines.push(format!("=== {} ===", c.name));
    lines.push(format!("{} {} (Level {})", c.race, c.class, c.level));
    lines.push(format!("HP: {}/{}", c.current_hp, c.max_hp));
    lines.push(format!("Speed: {} ft", c.speed));
    lines.push(format!("Proficiency Bonus: +{}", c.proficiency_bonus()));
    lines.push(String::new());
    lines.push("Ability Scores:".to_string());
    for ability in crate::types::Ability::all() {
        let score = c.ability_scores.get(ability).copied().unwrap_or(10);
        let modifier = crate::types::Ability::modifier(score);
        let sign = if modifier >= 0 { "+" } else { "" };
        lines.push(format!("  {} {:2} ({}{})", ability, score, sign, modifier));
    }
    lines.push(String::new());
    if !c.skill_proficiencies.is_empty() {
        lines.push("Skill Proficiencies:".to_string());
        for skill in &c.skill_proficiencies {
            lines.push(format!("  {} (+{})", skill, c.skill_modifier(*skill)));
        }
    }
    if !c.traits.is_empty() {
        lines.push(String::new());
        lines.push(format!("Traits: {}", c.traits.join(", ")));
    }

    // Equipment
    lines.push(String::new());
    lines.push("Equipment:".to_string());

    let main_hand_str = match c.equipped.main_hand {
        Some(id) => format_weapon_slot(&state.world.items, id),
        None => "(empty)".to_string(),
    };
    lines.push(format!("  Main hand: {}", main_hand_str));

    let off_hand_str = match c.equipped.off_hand {
        Some(id) => format_equip_slot(&state.world.items, id),
        None => "(empty)".to_string(),
    };
    lines.push(format!("  Off hand:  {}", off_hand_str));

    let body_str = match c.equipped.body {
        Some(id) => format_armor_slot(&state.world.items, id),
        None => "(none)".to_string(),
    };
    lines.push(format!("  Body:      {}", body_str));

    // AC
    let ac = crate::equipment::calculate_ac(c, &state.world.items);
    lines.push(String::new());
    lines.push(format!("AC: {}", ac));

    if matches!(c.class, crate::character::class::Class::Barbarian) {
        lines.push(String::new());
        lines.push("Barbarian Features:".to_string());
        let rage_state = if c.class_features.rage_active {
            "active".to_string()
        } else if c.class_features.rage_uses_remaining > 0 {
            "ready".to_string()
        } else {
            "spent".to_string()
        };
        lines.push(format!(
            "  Rage: {} ({} use{} remaining)",
            rage_state,
            c.class_features.rage_uses_remaining,
            if c.class_features.rage_uses_remaining == 1 {
                ""
            } else {
                "s"
            }
        ));
        if c.equipped.body.is_none() {
            lines.push(
                "  Unarmored Defense: AC uses 10 + DEX modifier + CON modifier while unarmored."
                    .to_string(),
            );
        }
        lines.push(
            "  Signature combat options: rage, grapple <target>, shove <target>.".to_string(),
        );
    }

    if matches!(c.class, crate::character::class::Class::Monk) {
        lines.push(String::new());
        lines.push("Monk Features:".to_string());
        if c.level >= 2 {
            // Ki points = monk level; restored on short or long rest.
            let ki_max = c.level;
            lines.push(format!(
                "  Ki Points: {}/{} (restored on short or long rest)",
                c.class_features.ki_points_remaining, ki_max
            ));
            lines.push(
                "  Ki abilities (spend as bonus action in combat):".to_string(),
            );
            lines.push(
                "    - ki flurry [target]    : Flurry of Blows — 2 unarmed strikes (requires Attack action first)".to_string(),
            );
            lines.push(
                "    - ki patient defense    : Patient Defense — Dodge until next turn".to_string(),
            );
            lines.push(
                "    - ki step               : Step of the Wind — Disengage as bonus action".to_string(),
            );
            lines.push(
                "    - ki step dash          : Step of the Wind — Dash as bonus action".to_string(),
            );
        } else {
            lines.push(
                "  Ki: Not yet available (unlocks at level 2).".to_string(),
            );
        }
        if c.equipped.body.is_none() {
            lines.push(
                "  Unarmored Defense: AC uses 10 + DEX modifier + WIS modifier while unarmored."
                    .to_string(),
            );
        }
    }

    lines
}

/// Render a "condition applied" message. `target` is `None` for the player (uses
/// second-person) or `Some(name)` for an NPC/creature.
pub fn narrate_condition_applied(target: Option<&str>, condition_name: &str) -> String {
    match target {
        None => templates::CONDITION_APPLIED_SELF.replace("{condition}", condition_name),
        Some(name) => templates::CONDITION_APPLIED_OTHER
            .replace("{target}", name)
            .replace("{condition}", condition_name),
    }
}

/// Render a "condition saved" (save-ends success) message.
pub fn narrate_condition_saved(target: Option<&str>, condition_name: &str) -> String {
    match target {
        None => templates::CONDITION_SAVED_SELF.replace("{condition}", condition_name),
        Some(name) => templates::CONDITION_SAVED_OTHER
            .replace("{target}", name)
            .replace("{condition}", condition_name),
    }
}

/// Render a "condition expired" (duration ran out) message.
pub fn narrate_condition_expired(target: Option<&str>, condition_name: &str) -> String {
    match target {
        None => templates::CONDITION_EXPIRED_SELF.replace("{condition}", condition_name),
        Some(name) => templates::CONDITION_EXPIRED_OTHER
            .replace("{target}", name)
            .replace("{condition}", condition_name),
    }
}

/// Render "gained an exhaustion level" message. `lethal` should be true when the
/// new level is >= 6.
pub fn narrate_exhaustion_gained(target: Option<&str>, new_level: u32, lethal: bool) -> String {
    if lethal {
        match target {
            None => templates::EXHAUSTION_LETHAL_SELF.to_string(),
            Some(name) => templates::EXHAUSTION_LETHAL_OTHER.replace("{target}", name),
        }
    } else {
        let level = new_level.to_string();
        match target {
            None => templates::EXHAUSTION_GAINED_SELF.replace("{level}", &level),
            Some(name) => templates::EXHAUSTION_GAINED_OTHER
                .replace("{target}", name)
                .replace("{level}", &level),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_narrate_enter_location_includes_npc_barks() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = make_test_state_with_npcs();
        let loc = state.world.locations.get(&0).unwrap();
        let barks = vec![
            ("Aldric the Bold".to_string(), "\"Keep moving.\"".to_string()),
            (
                "Brenna the Wise".to_string(),
                "\"Fine goods here, if you're buying.\"".to_string(),
            ),
        ];
        let lines = narrate_enter_location(&mut rng, loc, &state, &barks);
        let joined = lines.join("\n");

        assert!(
            joined.contains("Aldric the Bold: \"Keep moving.\""),
            "Expected guard bark in output. Got:\n{}",
            joined
        );
        assert!(
            joined.contains("Brenna the Wise: \"Fine goods here, if you're buying.\""),
            "Expected merchant bark in output. Got:\n{}",
            joined
        );
    }

    #[test]
    fn test_narrate_enter_location_no_barks_when_empty() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = make_test_state_with_npcs();
        let loc = state.world.locations.get(&0).unwrap();
        let barks: Vec<(String, String)> = vec![];
        let lines = narrate_enter_location(&mut rng, loc, &state, &barks);
        // Should still have NPC names line but no bark lines
        let joined = lines.join("\n");
        assert!(
            joined.contains("You see"),
            "Expected NPC names line. Got:\n{}",
            joined
        );
        // No colon-prefixed bark lines
        assert!(
            !joined.contains("Aldric the Bold:"),
            "Expected no bark lines. Got:\n{}",
            joined
        );
    }

    #[test]
    fn test_narrate_look_includes_npc_barks() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = make_test_state_with_npcs();
        let loc = state.world.locations.get(&0).unwrap();
        let barks = vec![(
            "Aldric the Bold".to_string(),
            "\"Stay out of trouble.\"".to_string(),
        )];
        let lines = narrate_look(&mut rng, loc, &state, &barks);
        let joined = lines.join("\n");

        assert!(
            joined.contains("Aldric the Bold: \"Stay out of trouble.\""),
            "Expected bark in look output. Got:\n{}",
            joined
        );
    }

    fn make_test_state_with_npcs() -> crate::state::GameState {
        use crate::character::{class::Class, create_character, race::Race};
        use crate::state::*;
        use crate::types::Ability;
        use std::collections::{HashMap, HashSet};

        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 15);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 12);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);

        let character = create_character(
            "TestHero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![],
        );

        let mut npcs = HashMap::new();
        npcs.insert(
            0,
            Npc {
                id: 0,
                name: "Aldric the Bold".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Neutral,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: None,
                conditions: vec![],
            },
        );
        npcs.insert(
            1,
            Npc {
                id: 1,
                name: "Brenna the Wise".to_string(),
                role: NpcRole::Merchant,
                disposition: Disposition::Friendly,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: None,
                conditions: vec![],
            },
        );

        let mut locations = HashMap::new();
        locations.insert(
            0,
            Location {
                id: 0,
                name: "Guard Post".to_string(),
                description: "A dimly lit guard post.".to_string(),
                location_type: LocationType::Room,
                exits: HashMap::new(),
                npcs: vec![0, 1],
                items: Vec::new(),
                triggers: Vec::new(),
                light_level: LightLevel::Dim,
                room_features: Vec::new(),
            },
        );

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

    #[test]
    fn test_narrate_enter_location_shows_disposition_tags() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = make_test_state_with_npcs();
        let loc = state.world.locations.get(&0).unwrap();
        let barks: Vec<(String, String)> = vec![];
        let lines = narrate_enter_location(&mut rng, loc, &state, &barks);
        let joined = lines.join("\n");

        // Aldric the Bold is a Guard with Neutral disposition -> [neutral]
        assert!(
            joined.contains("Aldric the Bold [neutral]"),
            "Expected 'Aldric the Bold [neutral]' in room entry. Got:\n{}",
            joined
        );
        // Brenna the Wise is a Merchant -> [merchant]
        assert!(
            joined.contains("Brenna the Wise [merchant]"),
            "Expected 'Brenna the Wise [merchant]' in room entry. Got:\n{}",
            joined
        );
    }

    #[test]
    fn test_narrate_look_shows_disposition_tags() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = make_test_state_with_npcs();
        let loc = state.world.locations.get(&0).unwrap();
        let barks: Vec<(String, String)> = vec![];
        let lines = narrate_look(&mut rng, loc, &state, &barks);
        let joined = lines.join("\n");

        assert!(
            joined.contains("Aldric the Bold [neutral]"),
            "Expected 'Aldric the Bold [neutral]' in look. Got:\n{}",
            joined
        );
        assert!(
            joined.contains("Brenna the Wise [merchant]"),
            "Expected 'Brenna the Wise [merchant]' in look. Got:\n{}",
            joined
        );
    }

    #[test]
    fn test_narrate_condition_applied_self() {
        let text = narrate_condition_applied(None, "poisoned");
        assert_eq!(text, "You are poisoned!");
    }

    #[test]
    fn test_narrate_condition_applied_other() {
        let text = narrate_condition_applied(Some("the goblin"), "stunned");
        assert_eq!(text, "the goblin is stunned!");
    }

    #[test]
    fn test_narrate_condition_saved_self() {
        let text = narrate_condition_saved(None, "frightened");
        assert_eq!(text, "You shake off the frightened.");
    }

    #[test]
    fn test_narrate_condition_expired_other() {
        let text = narrate_condition_expired(Some("the spider"), "paralyzed");
        assert_eq!(text, "the spider is no longer paralyzed.");
    }

    #[test]
    fn test_narrate_exhaustion_gained_non_lethal() {
        let text = narrate_exhaustion_gained(None, 3, false);
        assert!(text.contains("level 3"));
        assert!(text.contains("exhaustion"));
    }

    #[test]
    fn test_narrate_exhaustion_gained_lethal() {
        let self_text = narrate_exhaustion_gained(None, 6, true);
        assert!(self_text.contains("level 6"));
        assert!(self_text.to_lowercase().contains("lifeless"));

        let other_text = narrate_exhaustion_gained(Some("Grik"), 6, true);
        assert!(other_text.starts_with("Grik"));
        assert!(other_text.to_lowercase().contains("lifeless"));
    }

    #[test]
    fn test_narrate_skill_check_success() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = CheckResult {
            roll: 15,
            modifier: 5,
            total: 20,
            dc: 15,
            success: true,
            natural_20: false,
            natural_1: false,
        };
        let text = narrate_skill_check(&mut rng, "Perception", &result);
        assert!(text.contains("Success"));
        assert!(text.contains("15"));
        assert!(text.contains("20"));
    }

    #[test]
    fn test_narrate_skill_check_failure() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = CheckResult {
            roll: 5,
            modifier: 2,
            total: 7,
            dc: 15,
            success: false,
            natural_20: false,
            natural_1: false,
        };
        let text = narrate_skill_check(&mut rng, "Stealth", &result);
        assert!(text.contains("Failure"));
    }
}
