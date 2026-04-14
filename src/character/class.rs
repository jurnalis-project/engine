// jurnalis-engine/src/character/class.rs
use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};

/// Per-class feature-use tracking. All fields default so older saves
/// deserialize cleanly.
///
/// Short-rest features (refresh on short OR long rest):
///   - `second_wind_available` (Fighter)
///
/// Long-rest features (refresh only on long rest):
///   - `action_surge_available` (Fighter)
///   - `arcane_recovery_used_today` (Wizard — true = already used today)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassFeatureState {
    #[serde(default = "default_true")]
    pub second_wind_available: bool,
    #[serde(default = "default_true")]
    pub action_surge_available: bool,
    #[serde(default)]
    pub arcane_recovery_used_today: bool,
}

fn default_true() -> bool { true }

impl Default for ClassFeatureState {
    fn default() -> Self {
        Self {
            second_wind_available: true,
            action_surge_available: true,
            arcane_recovery_used_today: false,
        }
    }
}

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

/// Describes the starting equipment loadout for a class.
/// Contains item names (matching SRD const table entries) categorized by slot.
pub struct StartingLoadout {
    pub main_hand: Option<&'static str>,
    pub off_hand: Option<&'static str>,
    pub body: Option<&'static str>,
    pub extra_inventory: &'static [&'static str],
}

impl Class {
    pub fn starting_loadout(&self) -> StartingLoadout {
        match self {
            Class::Fighter => StartingLoadout {
                main_hand: Some("Longsword"),
                off_hand: Some("Shield"),
                body: Some("Chain Mail"),
                extra_inventory: &[],
            },
            Class::Rogue => StartingLoadout {
                main_hand: Some("Shortsword"),
                off_hand: None,
                body: Some("Leather"),
                extra_inventory: &["Dagger"],
            },
            Class::Wizard => StartingLoadout {
                main_hand: Some("Quarterstaff"),
                off_hand: None,
                body: None,
                extra_inventory: &["Dagger"],
            },
        }
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

    #[test]
    fn test_fighter_starting_loadout() {
        let loadout = Class::Fighter.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Longsword"));
        assert_eq!(loadout.off_hand, Some("Shield"));
        assert_eq!(loadout.body, Some("Chain Mail"));
        assert!(loadout.extra_inventory.is_empty());
    }

    #[test]
    fn test_rogue_starting_loadout() {
        let loadout = Class::Rogue.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Shortsword"));
        assert_eq!(loadout.off_hand, None);
        assert_eq!(loadout.body, Some("Leather"));
        assert_eq!(loadout.extra_inventory, &["Dagger"]);
    }

    #[test]
    fn test_class_feature_state_defaults() {
        let f = ClassFeatureState::default();
        assert!(f.second_wind_available);
        assert!(f.action_surge_available);
        assert!(!f.arcane_recovery_used_today);
    }

    #[test]
    fn test_class_feature_state_missing_fields_deserialize_defaults() {
        // Simulate a save from before the field existed: use default().
        let json = "{}";
        let parsed: ClassFeatureState = serde_json::from_str(json).unwrap();
        assert!(parsed.second_wind_available);
        assert!(parsed.action_surge_available);
        assert!(!parsed.arcane_recovery_used_today);
    }

    #[test]
    fn test_wizard_starting_loadout() {
        let loadout = Class::Wizard.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Quarterstaff"));
        assert_eq!(loadout.off_hand, None);
        assert_eq!(loadout.body, None);
        assert_eq!(loadout.extra_inventory, &["Dagger"]);
    }
}
