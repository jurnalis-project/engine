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
        match self { Race::Human | Race::Elf => 30, Race::Dwarf => 25 }
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

    #[test]
    fn test_dwarf_speed_slower() {
        assert_eq!(Race::Dwarf.speed(), 25);
        assert_eq!(Race::Human.speed(), 30);
    }
}
