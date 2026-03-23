// jurnalis-engine/src/character/class.rs
use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Class { Fighter, Rogue, Wizard }

impl Class {
    pub fn all() -> &'static [Class] { &[Class::Fighter, Class::Rogue, Class::Wizard] }
    pub fn hit_die(&self) -> u32 { match self { Class::Fighter => 10, Class::Rogue => 8, Class::Wizard => 6 } }

    pub fn saving_throw_proficiencies(&self) -> Vec<Ability> {
        match self {
            Class::Fighter => vec![Ability::Strength, Ability::Constitution],
            Class::Rogue => vec![Ability::Dexterity, Ability::Intelligence],
            Class::Wizard => vec![Ability::Intelligence, Ability::Wisdom],
        }
    }

    pub fn skill_choices(&self) -> Vec<Skill> {
        match self {
            Class::Fighter => vec![Skill::Acrobatics, Skill::AnimalHandling, Skill::Athletics, Skill::History, Skill::Insight, Skill::Intimidation, Skill::Perception, Skill::Survival],
            Class::Rogue => vec![Skill::Acrobatics, Skill::Athletics, Skill::Deception, Skill::Insight, Skill::Intimidation, Skill::Investigation, Skill::Perception, Skill::Performance, Skill::Persuasion, Skill::SleightOfHand, Skill::Stealth],
            Class::Wizard => vec![Skill::Arcana, Skill::History, Skill::Insight, Skill::Investigation, Skill::Medicine, Skill::Religion],
        }
    }

    pub fn skill_choice_count(&self) -> usize { match self { Class::Fighter => 2, Class::Rogue => 4, Class::Wizard => 2 } }

    pub fn proficiency_bonus(level: u32) -> i32 {
        match level { 1..=4 => 2, 5..=8 => 3, 9..=12 => 4, 13..=16 => 5, 17..=20 => 6, _ => 2 }
    }
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Class::Fighter => write!(f, "Fighter"), Class::Rogue => write!(f, "Rogue"), Class::Wizard => write!(f, "Wizard") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn test_fighter_hit_die() { assert_eq!(Class::Fighter.hit_die(), 10); }
    #[test] fn test_rogue_gets_4_skills() { assert_eq!(Class::Rogue.skill_choice_count(), 4); }
    #[test]
    fn test_proficiency_bonus_scaling() {
        assert_eq!(Class::proficiency_bonus(1), 2);
        assert_eq!(Class::proficiency_bonus(5), 3);
        assert_eq!(Class::proficiency_bonus(9), 4);
        assert_eq!(Class::proficiency_bonus(13), 5);
        assert_eq!(Class::proficiency_bonus(17), 6);
    }
    #[test]
    fn test_wizard_saves() {
        let saves = Class::Wizard.saving_throw_proficiencies();
        assert!(saves.contains(&Ability::Intelligence));
        assert!(saves.contains(&Ability::Wisdom));
    }
}
