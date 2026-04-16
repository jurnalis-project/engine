// jurnalis-engine/src/character/race.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::Ability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Race { Human, Elf, Dwarf }

impl Race {
    pub fn all() -> &'static [Race] { &[Race::Human, Race::Elf, Race::Dwarf] }

    pub fn ability_bonuses(&self) -> HashMap<Ability, i32> {
        match self {
            Race::Human => Ability::all().iter().map(|&a| (a, 1)).collect(),
            Race::Elf => { let mut m = HashMap::new(); m.insert(Ability::Dexterity, 2); m }
            Race::Dwarf => { let mut m = HashMap::new(); m.insert(Ability::Constitution, 2); m }
        }
    }

    pub fn traits(&self) -> Vec<&'static str> {
        match self {
            Race::Human => vec!["Extra Language"],
            Race::Elf => vec!["Darkvision", "Fey Ancestry", "Trance"],
            Race::Dwarf => vec!["Darkvision", "Dwarven Resilience", "Stonecunning"],
        }
    }

    pub fn speed(&self) -> i32 {
        // Per 2024 SRD, all three currently-supported species walk at 30 ft.
        // Kept as an explicit match so future species (e.g. Goliath 35 ft,
        // Wood Elf 35 ft) slot in cleanly.
        match self {
            Race::Human | Race::Elf | Race::Dwarf => 30,
        }
    }
}

impl std::fmt::Display for Race {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { Race::Human => write!(f, "Human"), Race::Elf => write!(f, "Elf"), Race::Dwarf => write!(f, "Dwarf") }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_gets_all_bonuses() {
        let bonuses = Race::Human.ability_bonuses();
        assert_eq!(bonuses.len(), 6);
        for ability in Ability::all() { assert_eq!(bonuses[ability], 1); }
    }

    #[test]
    fn test_elf_gets_dex_bonus() {
        let bonuses = Race::Elf.ability_bonuses();
        assert_eq!(bonuses.get(&Ability::Dexterity), Some(&2));
        assert_eq!(bonuses.get(&Ability::Strength), None);
    }

    #[test]
    fn test_dwarf_gets_con_bonus() {
        let bonuses = Race::Dwarf.ability_bonuses();
        assert_eq!(bonuses.get(&Ability::Constitution), Some(&2));
    }

    // Hypothesis: the 2024 SRD sets Dwarf walking speed to 30 ft (same as Human/Elf).
    // The previous implementation hard-coded 25 ft from the 2014 SRD. This test
    // asserts the 2024-correct value and covers all three races.
    #[test]
    fn test_all_races_have_srd_2024_speed() {
        assert_eq!(Race::Human.speed(), 30);
        assert_eq!(Race::Elf.speed(), 30);
        assert_eq!(Race::Dwarf.speed(), 30);
    }
}
