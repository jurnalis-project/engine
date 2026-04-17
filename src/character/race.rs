// jurnalis-engine/src/character/race.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::Ability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Race {
    Human,
    Elf,
    Dwarf,
    Dragonborn,
    Gnome,
    Goliath,
    Halfling,
    Orc,
    Tiefling,
}

impl Race {
    pub fn all() -> &'static [Race] {
        &[
            Race::Human,
            Race::Elf,
            Race::Dwarf,
            Race::Dragonborn,
            Race::Gnome,
            Race::Goliath,
            Race::Halfling,
            Race::Orc,
            Race::Tiefling,
        ]
    }

    /// Ability score bonuses granted by this species. Legacy species (Human,
    /// Elf, Dwarf) retain their 2014 SRD bonuses. New 2024 SRD species return
    /// empty maps -- ability adjustments come from the background system.
    pub fn ability_bonuses(&self) -> HashMap<Ability, i32> {
        match self {
            Race::Human => Ability::all().iter().map(|&a| (a, 1)).collect(),
            Race::Elf => { let mut m = HashMap::new(); m.insert(Ability::Dexterity, 2); m }
            Race::Dwarf => { let mut m = HashMap::new(); m.insert(Ability::Constitution, 2); m }
            // 2024 SRD species: no racial ability bonuses.
            Race::Dragonborn | Race::Gnome | Race::Goliath
            | Race::Halfling | Race::Orc | Race::Tiefling => HashMap::new(),
        }
    }

    /// Base racial traits (before subrace selection). For species with
    /// subraces, lineage-specific traits are appended by `traits_with_subrace`.
    pub fn traits(&self) -> Vec<&'static str> {
        match self {
            Race::Human => vec!["Extra Language"],
            Race::Elf => vec!["Darkvision", "Fey Ancestry", "Keen Senses", "Trance"],
            Race::Dwarf => vec!["Darkvision", "Dwarven Resilience", "Dwarven Toughness", "Stonecunning"],
            Race::Dragonborn => vec!["Darkvision", "Draconic Ancestry", "Breath Weapon", "Damage Resistance", "Draconic Flight"],
            Race::Gnome => vec!["Darkvision", "Gnomish Cunning"],
            Race::Goliath => vec!["Giant Ancestry", "Large Form", "Powerful Build"],
            Race::Halfling => vec!["Brave", "Halfling Nimbleness", "Luck", "Naturally Stealthy"],
            Race::Orc => vec!["Darkvision", "Adrenaline Rush", "Relentless Endurance"],
            Race::Tiefling => vec!["Darkvision", "Fiendish Legacy", "Otherworldly Presence"],
        }
    }

    /// Walking speed in feet per the 2024 SRD.
    pub fn speed(&self) -> i32 {
        match self {
            Race::Goliath => 35,
            Race::Human | Race::Elf | Race::Dwarf | Race::Dragonborn
            | Race::Gnome | Race::Halfling | Race::Orc | Race::Tiefling => 30,
        }
    }

    /// Whether this species requires a subrace/lineage selection step during
    /// character creation.
    pub fn has_subraces(&self) -> bool {
        matches!(self, Race::Elf | Race::Dragonborn | Race::Gnome | Race::Goliath | Race::Tiefling)
    }

    /// Available subrace/lineage options for this species. Returns an empty
    /// slice for species without subraces.
    pub fn subrace_options(&self) -> &'static [&'static str] {
        match self {
            Race::Elf => &["Drow", "High Elf", "Wood Elf"],
            Race::Dragonborn => &[
                "Black", "Blue", "Brass", "Bronze", "Copper",
                "Gold", "Green", "Red", "Silver", "White",
            ],
            Race::Gnome => &["Forest Gnome", "Rock Gnome"],
            Race::Goliath => &["Cloud", "Fire", "Frost", "Hill", "Stone", "Storm"],
            Race::Tiefling => &["Abyssal", "Chthonic", "Infernal"],
            _ => &[],
        }
    }

    /// Prompt label for the subrace selection step (e.g. "Elven Lineage").
    pub fn subrace_label(&self) -> &'static str {
        match self {
            Race::Elf => "Elven Lineage",
            Race::Dragonborn => "Draconic Ancestry",
            Race::Gnome => "Gnomish Lineage",
            Race::Goliath => "Giant Ancestry",
            Race::Tiefling => "Fiendish Legacy",
            _ => "",
        }
    }

    /// Short description for each subrace option shown in the creation prompt.
    pub fn subrace_description(subrace: &str) -> &'static str {
        match subrace {
            // Elf
            "Drow" => "Darkvision 120 ft, Dancing Lights cantrip",
            "High Elf" => "Prestidigitation cantrip",
            "Wood Elf" => "Speed 35 ft, Druidcraft cantrip",
            // Dragonborn
            "Black" => "Acid damage",
            "Blue" => "Lightning damage",
            "Brass" => "Fire damage",
            "Bronze" => "Lightning damage",
            "Copper" => "Acid damage",
            "Gold" => "Fire damage",
            "Green" => "Poison damage",
            "Red" => "Fire damage",
            "Silver" => "Cold damage",
            "White" => "Cold damage",
            // Gnome
            "Forest Gnome" => "Minor Illusion, Speak with Animals",
            "Rock Gnome" => "Mending, Prestidigitation",
            // Goliath
            "Cloud" => "Cloud's Jaunt (teleport 30 ft)",
            "Fire" => "Fire's Burn (bonus 1d10 fire damage)",
            "Frost" => "Frost's Chill (bonus 1d6 cold damage, -10 ft speed)",
            "Hill" => "Hill's Tumble (knock prone)",
            "Stone" => "Stone's Endurance (damage reduction)",
            "Storm" => "Storm's Thunder (1d8 thunder retaliation)",
            // Tiefling
            "Abyssal" => "Poison Resistance, Poison Spray cantrip",
            "Chthonic" => "Necrotic Resistance, Chill Touch cantrip",
            "Infernal" => "Fire Resistance, Fire Bolt cantrip",
            _ => "",
        }
    }

    /// Speed override for specific subraces (e.g. Wood Elf walks at 35 ft).
    /// Returns None if the subrace does not change the base speed.
    pub fn subrace_speed_override(subrace: &str) -> Option<i32> {
        match subrace {
            "Wood Elf" => Some(35),
            _ => None,
        }
    }

    /// Extra traits granted by a specific subrace/lineage selection.
    pub fn subrace_traits(subrace: &str) -> Vec<&'static str> {
        match subrace {
            // Elf
            "Drow" => vec!["Darkvision 120 ft", "Dancing Lights cantrip"],
            "High Elf" => vec!["Prestidigitation cantrip"],
            "Wood Elf" => vec!["Speed 35 ft", "Druidcraft cantrip"],
            // Dragonborn -- damage type noted as trait text
            "Black" => vec!["Acid Breath Weapon", "Acid Resistance"],
            "Blue" => vec!["Lightning Breath Weapon", "Lightning Resistance"],
            "Brass" => vec!["Fire Breath Weapon", "Fire Resistance"],
            "Bronze" => vec!["Lightning Breath Weapon", "Lightning Resistance"],
            "Copper" => vec!["Acid Breath Weapon", "Acid Resistance"],
            "Gold" => vec!["Fire Breath Weapon", "Fire Resistance"],
            "Green" => vec!["Poison Breath Weapon", "Poison Resistance"],
            "Red" => vec!["Fire Breath Weapon", "Fire Resistance"],
            "Silver" => vec!["Cold Breath Weapon", "Cold Resistance"],
            "White" => vec!["Cold Breath Weapon", "Cold Resistance"],
            // Gnome
            "Forest Gnome" => vec!["Minor Illusion cantrip", "Speak with Animals"],
            "Rock Gnome" => vec!["Mending cantrip", "Prestidigitation cantrip"],
            // Goliath
            "Cloud" => vec!["Cloud's Jaunt"],
            "Fire" => vec!["Fire's Burn"],
            "Frost" => vec!["Frost's Chill"],
            "Hill" => vec!["Hill's Tumble"],
            "Stone" => vec!["Stone's Endurance"],
            "Storm" => vec!["Storm's Thunder"],
            // Tiefling
            "Abyssal" => vec!["Poison Resistance", "Poison Spray cantrip"],
            "Chthonic" => vec!["Necrotic Resistance", "Chill Touch cantrip"],
            "Infernal" => vec!["Fire Resistance", "Fire Bolt cantrip"],
            _ => vec![],
        }
    }
}

impl std::fmt::Display for Race {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Race::Human => write!(f, "Human"),
            Race::Elf => write!(f, "Elf"),
            Race::Dwarf => write!(f, "Dwarf"),
            Race::Dragonborn => write!(f, "Dragonborn"),
            Race::Gnome => write!(f, "Gnome"),
            Race::Goliath => write!(f, "Goliath"),
            Race::Halfling => write!(f, "Halfling"),
            Race::Orc => write!(f, "Orc"),
            Race::Tiefling => write!(f, "Tiefling"),
        }
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
    fn test_all_races_have_srd_2024_speed() {
        assert_eq!(Race::Human.speed(), 30);
        assert_eq!(Race::Elf.speed(), 30);
        assert_eq!(Race::Dwarf.speed(), 30);
    }

    // ---- New species ----

    #[test]
    fn test_race_all_returns_nine_species() {
        assert_eq!(Race::all().len(), 9);
    }

    #[test]
    fn test_new_species_have_empty_ability_bonuses() {
        for race in [Race::Dragonborn, Race::Gnome, Race::Goliath, Race::Halfling, Race::Orc, Race::Tiefling] {
            let bonuses = race.ability_bonuses();
            assert!(bonuses.is_empty(), "{:?} should have empty ability bonuses (2024 SRD)", race);
        }
    }

    #[test]
    fn test_goliath_speed_is_35() {
        assert_eq!(Race::Goliath.speed(), 35);
    }

    #[test]
    fn test_new_species_speed_30() {
        for race in [Race::Dragonborn, Race::Gnome, Race::Halfling, Race::Orc, Race::Tiefling] {
            assert_eq!(race.speed(), 30, "{:?} should have 30 ft speed", race);
        }
    }

    #[test]
    fn test_dragonborn_traits() {
        let traits = Race::Dragonborn.traits();
        assert!(traits.contains(&"Darkvision"));
        assert!(traits.contains(&"Breath Weapon"));
        assert!(traits.contains(&"Damage Resistance"));
        assert!(traits.contains(&"Draconic Ancestry"));
        assert!(traits.contains(&"Draconic Flight"));
    }

    #[test]
    fn test_gnome_traits() {
        let traits = Race::Gnome.traits();
        assert!(traits.contains(&"Darkvision"));
        assert!(traits.contains(&"Gnomish Cunning"));
    }

    #[test]
    fn test_goliath_traits() {
        let traits = Race::Goliath.traits();
        assert!(traits.contains(&"Giant Ancestry"));
        assert!(traits.contains(&"Large Form"));
        assert!(traits.contains(&"Powerful Build"));
    }

    #[test]
    fn test_halfling_traits() {
        let traits = Race::Halfling.traits();
        assert!(traits.contains(&"Brave"));
        assert!(traits.contains(&"Halfling Nimbleness"));
        assert!(traits.contains(&"Luck"));
        assert!(traits.contains(&"Naturally Stealthy"));
    }

    #[test]
    fn test_orc_traits() {
        let traits = Race::Orc.traits();
        assert!(traits.contains(&"Darkvision"));
        assert!(traits.contains(&"Adrenaline Rush"));
        assert!(traits.contains(&"Relentless Endurance"));
    }

    #[test]
    fn test_tiefling_traits() {
        let traits = Race::Tiefling.traits();
        assert!(traits.contains(&"Darkvision"));
        assert!(traits.contains(&"Fiendish Legacy"));
        assert!(traits.contains(&"Otherworldly Presence"));
    }

    #[test]
    fn test_has_subraces() {
        assert!(Race::Elf.has_subraces());
        assert!(Race::Dragonborn.has_subraces());
        assert!(Race::Gnome.has_subraces());
        assert!(Race::Goliath.has_subraces());
        assert!(Race::Tiefling.has_subraces());
        // No subraces
        assert!(!Race::Human.has_subraces());
        assert!(!Race::Dwarf.has_subraces());
        assert!(!Race::Halfling.has_subraces());
        assert!(!Race::Orc.has_subraces());
    }

    #[test]
    fn test_elf_subrace_options() {
        let opts = Race::Elf.subrace_options();
        assert_eq!(opts, &["Drow", "High Elf", "Wood Elf"]);
    }

    #[test]
    fn test_dragonborn_subrace_options_has_ten() {
        assert_eq!(Race::Dragonborn.subrace_options().len(), 10);
    }

    #[test]
    fn test_gnome_subrace_options() {
        let opts = Race::Gnome.subrace_options();
        assert_eq!(opts, &["Forest Gnome", "Rock Gnome"]);
    }

    #[test]
    fn test_goliath_subrace_options() {
        let opts = Race::Goliath.subrace_options();
        assert_eq!(opts.len(), 6);
    }

    #[test]
    fn test_tiefling_subrace_options() {
        let opts = Race::Tiefling.subrace_options();
        assert_eq!(opts, &["Abyssal", "Chthonic", "Infernal"]);
    }

    #[test]
    fn test_no_subrace_options_for_human() {
        assert!(Race::Human.subrace_options().is_empty());
    }

    #[test]
    fn test_wood_elf_speed_override() {
        assert_eq!(Race::subrace_speed_override("Wood Elf"), Some(35));
        assert_eq!(Race::subrace_speed_override("Drow"), None);
        assert_eq!(Race::subrace_speed_override("High Elf"), None);
    }

    #[test]
    fn test_subrace_traits_wood_elf() {
        let traits = Race::subrace_traits("Wood Elf");
        assert!(traits.contains(&"Speed 35 ft"));
        assert!(traits.contains(&"Druidcraft cantrip"));
    }

    #[test]
    fn test_subrace_traits_red_dragonborn() {
        let traits = Race::subrace_traits("Red");
        assert!(traits.contains(&"Fire Breath Weapon"));
        assert!(traits.contains(&"Fire Resistance"));
    }

    #[test]
    fn test_subrace_traits_infernal_tiefling() {
        let traits = Race::subrace_traits("Infernal");
        assert!(traits.contains(&"Fire Resistance"));
        assert!(traits.contains(&"Fire Bolt cantrip"));
    }

    #[test]
    fn test_subrace_traits_unknown_returns_empty() {
        assert!(Race::subrace_traits("Nonexistent").is_empty());
    }

    #[test]
    fn test_display_new_species() {
        assert_eq!(Race::Dragonborn.to_string(), "Dragonborn");
        assert_eq!(Race::Gnome.to_string(), "Gnome");
        assert_eq!(Race::Goliath.to_string(), "Goliath");
        assert_eq!(Race::Halfling.to_string(), "Halfling");
        assert_eq!(Race::Orc.to_string(), "Orc");
        assert_eq!(Race::Tiefling.to_string(), "Tiefling");
    }

    #[test]
    fn test_all_species_serde_roundtrip() {
        for race in Race::all() {
            let json = serde_json::to_string(race).unwrap();
            let back: Race = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, race);
        }
    }

    #[test]
    fn test_subrace_label() {
        assert_eq!(Race::Elf.subrace_label(), "Elven Lineage");
        assert_eq!(Race::Dragonborn.subrace_label(), "Draconic Ancestry");
        assert_eq!(Race::Gnome.subrace_label(), "Gnomish Lineage");
        assert_eq!(Race::Goliath.subrace_label(), "Giant Ancestry");
        assert_eq!(Race::Tiefling.subrace_label(), "Fiendish Legacy");
        assert_eq!(Race::Human.subrace_label(), "");
    }
}
