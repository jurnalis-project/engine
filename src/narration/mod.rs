pub mod templates;

use rand::Rng;
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
