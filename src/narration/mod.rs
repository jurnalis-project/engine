pub mod templates;

use rand::Rng;
use std::collections::HashMap;
use crate::state::{GameState, Location};
use crate::rules::checks::CheckResult;

pub fn narrate_enter_location(rng: &mut impl Rng, location: &Location, state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();

    // Location description
    let template = templates::pick(rng, templates::ENTER_LOCATION);
    lines.push(
        template
            .replace("{name}", &location.name)
            .replace("{description}", &location.description)
    );

    // Exits
    if !location.exits.is_empty() {
        let exit_names: Vec<String> = location.exits.keys().map(|d| d.to_string()).collect();
        lines.push(templates::EXITS.replace("{exits}", &exit_names.join(", ")));
    }

    // NPCs
    if !location.npcs.is_empty() {
        let npc_names: Vec<String> = location.npcs.iter()
            .filter_map(|id| state.world.npcs.get(id))
            .map(|npc| npc.name.clone())
            .collect();
        if !npc_names.is_empty() {
            lines.push(templates::NPCS_PRESENT.replace("{npcs}", &npc_names.join(", ")));
        }
    }

    // Items
    if !location.items.is_empty() {
        let item_names: Vec<String> = location.items.iter()
            .filter_map(|id| state.world.items.get(id))
            .filter(|item| !item.carried_by_player)
            .map(|item| item.name.clone())
            .collect();
        if !item_names.is_empty() {
            lines.push(templates::ITEMS_PRESENT.replace("{items}", &item_names.join(", ")));
        }
    }

    lines
}

pub fn narrate_look(rng: &mut impl Rng, location: &Location, state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();

    let template = templates::pick(rng, templates::LOOK_LOCATION);
    lines.push(
        template
            .replace("{name}", &location.name)
            .replace("{description}", &location.description)
    );

    if !location.exits.is_empty() {
        let exit_names: Vec<String> = location.exits.keys().map(|d| d.to_string()).collect();
        lines.push(templates::EXITS.replace("{exits}", &exit_names.join(", ")));
    }

    if !location.npcs.is_empty() {
        let npc_names: Vec<String> = location.npcs.iter()
            .filter_map(|id| state.world.npcs.get(id))
            .map(|npc| npc.name.clone())
            .collect();
        if !npc_names.is_empty() {
            lines.push(templates::NPCS_PRESENT.replace("{npcs}", &npc_names.join(", ")));
        }
    }

    if !location.items.is_empty() {
        let item_names: Vec<String> = location.items.iter()
            .filter_map(|id| state.world.items.get(id))
            .filter(|item| !item.carried_by_player)
            .map(|item| item.name.clone())
            .collect();
        if !item_names.is_empty() {
            lines.push(templates::ITEMS_PRESENT.replace("{items}", &item_names.join(", ")));
        }
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

fn format_weapon_slot(items: &HashMap<crate::types::ItemId, crate::state::Item>, id: crate::types::ItemId) -> String {
    match items.get(&id) {
        Some(item) => {
            if let crate::state::ItemType::Weapon { damage_dice, damage_die, damage_type, versatile_die, .. } = &item.item_type {
                if *damage_dice == 0 {
                    return item.name.clone(); // Net, etc.
                }
                let base = format!("{} ({}d{} {}", item.name, damage_dice, damage_die, damage_type);
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

fn format_armor_slot(items: &HashMap<crate::types::ItemId, crate::state::Item>, id: crate::types::ItemId) -> String {
    match items.get(&id) {
        Some(item) => {
            if let crate::state::ItemType::Armor { base_ac, stealth_disadvantage, .. } = &item.item_type {
                let disadv = if *stealth_disadvantage { ", stealth disadvantage" } else { "" };
                format!("{} (AC {}{})", item.name, base_ac, disadv)
            } else {
                item.name.clone()
            }
        }
        None => "(none)".to_string(),
    }
}

fn format_equip_slot(items: &HashMap<crate::types::ItemId, crate::state::Item>, id: crate::types::ItemId) -> String {
    match items.get(&id) {
        Some(item) => {
            match &item.item_type {
                crate::state::ItemType::Armor { category: crate::state::ArmorCategory::Shield, base_ac, .. } => {
                    format!("{} (+{} AC)", item.name, base_ac)
                }
                crate::state::ItemType::Weapon { .. } => format_weapon_slot(items, id),
                _ => item.name.clone(),
            }
        }
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

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

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
