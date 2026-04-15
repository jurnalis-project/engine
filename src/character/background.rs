// jurnalis-engine/src/character/background.rs
//
// SRD 2024 character backgrounds. Each background grants ability score options,
// two skill proficiencies, a tool proficiency, a language, an origin feat, and
// a starting equipment package. See docs/specs/background-system.md.

use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};

/// The 16 SRD 2024 character backgrounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Background {
    Acolyte,
    Artisan,
    Charlatan,
    Criminal,
    Entertainer,
    Farmer,
    Guard,
    Guide,
    Hermit,
    Merchant,
    Noble,
    Sage,
    Sailor,
    Scribe,
    Soldier,
    Wayfarer,
}

impl Default for Background {
    /// Default for older saves that predate the `background` field.
    fn default() -> Self { Background::Acolyte }
}

impl Background {
    pub fn all() -> &'static [Background] {
        &[
            Background::Acolyte, Background::Artisan, Background::Charlatan,
            Background::Criminal, Background::Entertainer, Background::Farmer,
            Background::Guard, Background::Guide, Background::Hermit,
            Background::Merchant, Background::Noble, Background::Sage,
            Background::Sailor, Background::Scribe, Background::Soldier,
            Background::Wayfarer,
        ]
    }

    /// The three abilities listed for this background. The player can either
    /// increase one by +2 and another by +1, or increase all three by +1.
    pub fn ability_options(&self) -> [Ability; 3] {
        match self {
            Background::Acolyte     => [Ability::Intelligence, Ability::Wisdom, Ability::Charisma],
            Background::Artisan     => [Ability::Strength, Ability::Dexterity, Ability::Intelligence],
            Background::Charlatan   => [Ability::Dexterity, Ability::Constitution, Ability::Charisma],
            Background::Criminal    => [Ability::Dexterity, Ability::Constitution, Ability::Intelligence],
            Background::Entertainer => [Ability::Strength, Ability::Dexterity, Ability::Charisma],
            Background::Farmer      => [Ability::Strength, Ability::Constitution, Ability::Wisdom],
            Background::Guard       => [Ability::Strength, Ability::Intelligence, Ability::Wisdom],
            Background::Guide       => [Ability::Dexterity, Ability::Constitution, Ability::Wisdom],
            Background::Hermit      => [Ability::Constitution, Ability::Wisdom, Ability::Charisma],
            Background::Merchant    => [Ability::Constitution, Ability::Intelligence, Ability::Charisma],
            Background::Noble       => [Ability::Strength, Ability::Intelligence, Ability::Charisma],
            Background::Sage        => [Ability::Constitution, Ability::Intelligence, Ability::Wisdom],
            Background::Sailor      => [Ability::Strength, Ability::Dexterity, Ability::Wisdom],
            Background::Scribe      => [Ability::Dexterity, Ability::Intelligence, Ability::Wisdom],
            Background::Soldier     => [Ability::Strength, Ability::Dexterity, Ability::Constitution],
            Background::Wayfarer    => [Ability::Dexterity, Ability::Wisdom, Ability::Charisma],
        }
    }

    /// Two skill proficiencies granted by this background.
    pub fn skill_proficiencies(&self) -> [Skill; 2] {
        match self {
            Background::Acolyte     => [Skill::Insight, Skill::Religion],
            Background::Artisan     => [Skill::Investigation, Skill::Persuasion],
            Background::Charlatan   => [Skill::Deception, Skill::SleightOfHand],
            Background::Criminal    => [Skill::SleightOfHand, Skill::Stealth],
            Background::Entertainer => [Skill::Acrobatics, Skill::Performance],
            Background::Farmer      => [Skill::AnimalHandling, Skill::Nature],
            Background::Guard       => [Skill::Athletics, Skill::Perception],
            Background::Guide       => [Skill::Stealth, Skill::Survival],
            Background::Hermit      => [Skill::Medicine, Skill::Religion],
            Background::Merchant    => [Skill::AnimalHandling, Skill::Persuasion],
            Background::Noble       => [Skill::History, Skill::Persuasion],
            Background::Sage        => [Skill::Arcana, Skill::History],
            Background::Sailor      => [Skill::Acrobatics, Skill::Perception],
            Background::Scribe      => [Skill::Investigation, Skill::Perception],
            Background::Soldier     => [Skill::Athletics, Skill::Intimidation],
            Background::Wayfarer    => [Skill::Insight, Skill::Stealth],
        }
    }

    /// Single tool proficiency name. Kept as a string because the tool system
    /// is not yet modelled as enums (pending issue #42).
    pub fn tool_proficiency(&self) -> &'static str {
        match self {
            Background::Acolyte     => "Calligrapher's Supplies",
            Background::Artisan     => "Artisan's Tools",
            Background::Charlatan   => "Forgery Kit",
            Background::Criminal    => "Thieves' Tools",
            Background::Entertainer => "Musical Instrument",
            Background::Farmer      => "Carpenter's Tools",
            Background::Guard       => "Gaming Set",
            Background::Guide       => "Cartographer's Tools",
            Background::Hermit      => "Herbalism Kit",
            Background::Merchant    => "Navigator's Tools",
            Background::Noble       => "Gaming Set",
            Background::Sage        => "Calligrapher's Supplies",
            Background::Sailor      => "Navigator's Tools",
            Background::Scribe      => "Calligrapher's Supplies",
            Background::Soldier     => "Gaming Set",
            Background::Wayfarer    => "Thieves' Tools",
        }
    }

    /// A standard language granted by this background (Common is always known).
    pub fn language(&self) -> &'static str {
        match self {
            Background::Acolyte     => "Celestial",
            Background::Artisan     => "Dwarvish",
            Background::Charlatan   => "Thieves' Cant",
            Background::Criminal    => "Thieves' Cant",
            Background::Entertainer => "Elvish",
            Background::Farmer      => "Halfling",
            Background::Guard       => "Dwarvish",
            Background::Guide       => "Elvish",
            Background::Hermit      => "Druidic",
            Background::Merchant    => "Gnomish",
            Background::Noble       => "Elvish",
            Background::Sage        => "Draconic",
            Background::Sailor      => "Primordial",
            Background::Scribe      => "Dwarvish",
            Background::Soldier     => "Orc",
            Background::Wayfarer    => "Halfling",
        }
    }

    /// The Origin feat granted at character creation. Recorded as a string
    /// trait until issue #28 lands.
    pub fn origin_feat(&self) -> &'static str {
        match self {
            Background::Acolyte     => "Magic Initiate (Cleric)",
            Background::Artisan     => "Crafter",
            Background::Charlatan   => "Skilled",
            Background::Criminal    => "Alert",
            Background::Entertainer => "Musician",
            Background::Farmer      => "Tough",
            Background::Guard       => "Alert",
            Background::Guide       => "Magic Initiate (Druid)",
            Background::Hermit      => "Healer",
            Background::Merchant    => "Lucky",
            Background::Noble       => "Skilled",
            Background::Sage        => "Magic Initiate (Wizard)",
            Background::Sailor      => "Tavern Brawler",
            Background::Scribe      => "Skilled",
            Background::Soldier     => "Savage Attacker",
            Background::Wayfarer    => "Lucky",
        }
    }

    /// Starting equipment package (option A). Each entry is a simple item name.
    /// Items that do not exist in the SRD weapon/armor tables are skipped at
    /// grant time (handled in the orchestrator).
    pub fn starting_equipment(&self) -> &'static [&'static str] {
        match self {
            Background::Acolyte     => &["Holy Symbol", "Book (prayers)", "Robe", "Calligrapher's Supplies"],
            Background::Artisan     => &["Artisan's Tools", "Traveler's Clothes"],
            Background::Charlatan   => &["Fine Clothes", "Forgery Kit"],
            Background::Criminal    => &["Dagger", "Dagger", "Thieves' Tools", "Crowbar", "Traveler's Clothes"],
            Background::Entertainer => &["Musical Instrument", "Costume", "Traveler's Clothes"],
            Background::Farmer      => &["Sickle", "Carpenter's Tools", "Traveler's Clothes"],
            Background::Guard       => &["Spear", "Light Crossbow", "Gaming Set", "Traveler's Clothes"],
            Background::Guide       => &["Shortbow", "Cartographer's Tools", "Traveler's Clothes"],
            Background::Hermit      => &["Quarterstaff", "Herbalism Kit", "Traveler's Clothes"],
            Background::Merchant    => &["Navigator's Tools", "Traveler's Clothes"],
            Background::Noble       => &["Gaming Set", "Fine Clothes"],
            Background::Sage        => &["Quarterstaff", "Book (history)", "Robe", "Calligrapher's Supplies"],
            Background::Sailor      => &["Dagger", "Navigator's Tools", "Traveler's Clothes"],
            Background::Scribe      => &["Calligrapher's Supplies", "Fine Clothes"],
            Background::Soldier     => &["Spear", "Shortbow", "Gaming Set", "Traveler's Clothes"],
            Background::Wayfarer    => &["Dagger", "Thieves' Tools", "Traveler's Clothes"],
        }
    }
}

impl std::fmt::Display for Background {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Background::Acolyte => "Acolyte",
            Background::Artisan => "Artisan",
            Background::Charlatan => "Charlatan",
            Background::Criminal => "Criminal",
            Background::Entertainer => "Entertainer",
            Background::Farmer => "Farmer",
            Background::Guard => "Guard",
            Background::Guide => "Guide",
            Background::Hermit => "Hermit",
            Background::Merchant => "Merchant",
            Background::Noble => "Noble",
            Background::Sage => "Sage",
            Background::Sailor => "Sailor",
            Background::Scribe => "Scribe",
            Background::Soldier => "Soldier",
            Background::Wayfarer => "Wayfarer",
        };
        write!(f, "{}", s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_returns_sixteen_backgrounds() {
        assert_eq!(Background::all().len(), 16);
    }

    #[test]
    fn test_default_is_acolyte() {
        assert_eq!(Background::default(), Background::Acolyte);
    }

    #[test]
    fn test_acolyte_stats() {
        let bg = Background::Acolyte;
        assert_eq!(bg.ability_options(), [Ability::Intelligence, Ability::Wisdom, Ability::Charisma]);
        assert_eq!(bg.skill_proficiencies(), [Skill::Insight, Skill::Religion]);
        assert_eq!(bg.tool_proficiency(), "Calligrapher's Supplies");
        assert_eq!(bg.origin_feat(), "Magic Initiate (Cleric)");
    }

    #[test]
    fn test_criminal_stats() {
        let bg = Background::Criminal;
        assert_eq!(bg.ability_options(), [Ability::Dexterity, Ability::Constitution, Ability::Intelligence]);
        assert_eq!(bg.skill_proficiencies(), [Skill::SleightOfHand, Skill::Stealth]);
        assert_eq!(bg.tool_proficiency(), "Thieves' Tools");
        assert_eq!(bg.origin_feat(), "Alert");
    }

    #[test]
    fn test_sage_stats() {
        let bg = Background::Sage;
        assert_eq!(bg.ability_options(), [Ability::Constitution, Ability::Intelligence, Ability::Wisdom]);
        assert_eq!(bg.skill_proficiencies(), [Skill::Arcana, Skill::History]);
        assert_eq!(bg.origin_feat(), "Magic Initiate (Wizard)");
    }

    #[test]
    fn test_soldier_stats() {
        let bg = Background::Soldier;
        assert_eq!(bg.ability_options(), [Ability::Strength, Ability::Dexterity, Ability::Constitution]);
        assert_eq!(bg.skill_proficiencies(), [Skill::Athletics, Skill::Intimidation]);
        assert_eq!(bg.origin_feat(), "Savage Attacker");
    }

    #[test]
    fn test_every_background_has_complete_data() {
        for &bg in Background::all() {
            // These panic if any variant is missing; also catches duplicate
            // skill entries that would violate [_; 2] distinctness.
            let abilities = bg.ability_options();
            assert_eq!(abilities.len(), 3);
            let skills = bg.skill_proficiencies();
            assert_ne!(skills[0], skills[1], "Background {:?} has duplicate skill proficiencies", bg);
            assert!(!bg.tool_proficiency().is_empty(), "Background {:?} has empty tool proficiency", bg);
            assert!(!bg.language().is_empty(), "Background {:?} has empty language", bg);
            assert!(!bg.origin_feat().is_empty(), "Background {:?} has empty origin feat", bg);
            assert!(!bg.starting_equipment().is_empty(), "Background {:?} has empty starting equipment", bg);
        }
    }

    #[test]
    fn test_display_names_match_variants() {
        assert_eq!(Background::Acolyte.to_string(), "Acolyte");
        assert_eq!(Background::Wayfarer.to_string(), "Wayfarer");
    }

    #[test]
    fn test_serialization_roundtrip() {
        for &bg in Background::all() {
            let json = serde_json::to_string(&bg).unwrap();
            let loaded: Background = serde_json::from_str(&json).unwrap();
            assert_eq!(loaded, bg);
        }
    }
}
