// jurnalis-engine/src/character/mod.rs
pub mod race;
pub mod class;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;
use crate::types::{Ability, Skill, ItemId};
use self::race::Race;
use self::class::Class;
use crate::rules::dice::roll_4d6_drop_lowest;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub race: Race,
    pub class: Class,
    pub level: u32,
    pub ability_scores: HashMap<Ability, i32>,
    pub skill_proficiencies: Vec<Skill>,
    pub save_proficiencies: Vec<Ability>,
    pub max_hp: i32,
    pub current_hp: i32,
    pub inventory: Vec<ItemId>,
    pub speed: i32,
    pub traits: Vec<String>,
}

impl Character {
    pub fn proficiency_bonus(&self) -> i32 { Class::proficiency_bonus(self.level) }
    pub fn ability_modifier(&self, ability: Ability) -> i32 {
        let score = self.ability_scores.get(&ability).copied().unwrap_or(10);
        Ability::modifier(score)
    }
    pub fn is_proficient_in_skill(&self, skill: Skill) -> bool { self.skill_proficiencies.contains(&skill) }
    pub fn is_proficient_in_save(&self, ability: Ability) -> bool { self.save_proficiencies.contains(&ability) }
    pub fn skill_modifier(&self, skill: Skill) -> i32 {
        let base = self.ability_modifier(skill.ability());
        if self.is_proficient_in_skill(skill) { base + self.proficiency_bonus() } else { base }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityScoreMethod { StandardArray, PointBuy, Random }

pub const STANDARD_ARRAY: [i32; 6] = [15, 14, 13, 12, 10, 8];

pub fn generate_random_scores(rng: &mut impl Rng) -> [i32; 6] {
    let mut scores = [0i32; 6];
    for score in scores.iter_mut() { *score = roll_4d6_drop_lowest(rng); }
    scores
}

const POINT_BUY_COSTS: [(i32, i32); 8] = [
    (8, 0), (9, 1), (10, 2), (11, 3), (12, 4), (13, 5), (14, 7), (15, 9),
];

pub fn point_buy_cost(score: i32) -> Option<i32> {
    POINT_BUY_COSTS.iter().find(|(s, _)| *s == score).map(|(_, c)| *c)
}

pub fn validate_point_buy(scores: &[i32; 6]) -> Result<(), String> {
    let mut total = 0;
    for &score in scores {
        match point_buy_cost(score) {
            Some(cost) => total += cost,
            None => return Err(format!("Score {} is out of range (8-15)", score)),
        }
    }
    if total != 27 { return Err(format!("Total cost is {} (must be 27)", total)); }
    Ok(())
}

pub fn calculate_hp(class: Class, con_modifier: i32, level: u32) -> i32 {
    let hit_die = class.hit_die() as i32;
    let first_level = hit_die + con_modifier;
    let per_level = (hit_die / 2) + 1 + con_modifier;
    let additional = per_level * (level as i32 - 1);
    (first_level + additional).max(1)
}

pub fn create_character(
    name: String, race: Race, class: Class,
    ability_scores: HashMap<Ability, i32>, skill_proficiencies: Vec<Skill>,
) -> Character {
    let mut final_scores = ability_scores;
    for (ability, bonus) in race.ability_bonuses() {
        *final_scores.entry(ability).or_insert(10) += bonus;
    }
    let con_mod = Ability::modifier(*final_scores.get(&Ability::Constitution).unwrap_or(&10));
    let hp = calculate_hp(class, con_mod, 1);
    let save_profs = class.saving_throw_proficiencies();
    let traits = race.traits().iter().map(|s| s.to_string()).collect();
    Character {
        name, race, class, level: 1,
        ability_scores: final_scores, skill_proficiencies,
        save_proficiencies: save_profs, max_hp: hp, current_hp: hp,
        inventory: Vec::new(), speed: race.speed(), traits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scores() -> HashMap<Ability, i32> {
        let mut m = HashMap::new();
        m.insert(Ability::Strength, 15); m.insert(Ability::Dexterity, 14);
        m.insert(Ability::Constitution, 13); m.insert(Ability::Intelligence, 12);
        m.insert(Ability::Wisdom, 10); m.insert(Ability::Charisma, 8);
        m
    }

    #[test]
    fn test_create_character_applies_racial_bonuses() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![Skill::Stealth]);
        assert_eq!(c.ability_scores[&Ability::Dexterity], 16);
    }

    #[test]
    fn test_create_character_hp() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert_eq!(c.max_hp, 12); assert_eq!(c.current_hp, 12);
    }

    #[test]
    fn test_skill_modifier_with_proficiency() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![Skill::Stealth]);
        assert_eq!(c.skill_modifier(Skill::Stealth), 5);
    }

    #[test]
    fn test_skill_modifier_without_proficiency() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![]);
        assert_eq!(c.skill_modifier(Skill::Stealth), 3);
    }

    #[test]
    fn test_random_scores_in_range() {
        use rand::SeedableRng; use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(42);
        let scores = generate_random_scores(&mut rng);
        for score in scores { assert!(score >= 3 && score <= 18, "Score {} out of range", score); }
    }

    #[test]
    fn test_calculate_hp_level_scaling() {
        assert_eq!(calculate_hp(Class::Fighter, 2, 1), 12);
        assert_eq!(calculate_hp(Class::Fighter, 2, 2), 20);
    }

    #[test]
    fn test_point_buy_valid() { assert!(validate_point_buy(&[15, 14, 13, 12, 10, 8]).is_ok()); }

    #[test]
    fn test_point_buy_wrong_total() { assert!(validate_point_buy(&[15, 15, 14, 8, 8, 8]).is_err()); }

    #[test]
    fn test_point_buy_out_of_range() {
        assert!(validate_point_buy(&[16, 14, 13, 12, 10, 8]).is_err());
        assert!(validate_point_buy(&[7, 14, 13, 12, 10, 8]).is_err());
    }

    #[test]
    fn test_point_buy_cost() {
        assert_eq!(point_buy_cost(8), Some(0));
        assert_eq!(point_buy_cost(15), Some(9));
        assert_eq!(point_buy_cost(16), None);
    }
}
