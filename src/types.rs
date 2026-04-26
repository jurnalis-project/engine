use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ability {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

impl Ability {
    pub fn all() -> &'static [Ability] {
        &[
            Ability::Strength,
            Ability::Dexterity,
            Ability::Constitution,
            Ability::Intelligence,
            Ability::Wisdom,
            Ability::Charisma,
        ]
    }

    pub fn modifier(score: i32) -> i32 {
        (score - 10).div_euclid(2)
    }
}

impl std::fmt::Display for Ability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ability::Strength => write!(f, "STR"),
            Ability::Dexterity => write!(f, "DEX"),
            Ability::Constitution => write!(f, "CON"),
            Ability::Intelligence => write!(f, "INT"),
            Ability::Wisdom => write!(f, "WIS"),
            Ability::Charisma => write!(f, "CHA"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Skill {
    Athletics,
    Acrobatics,
    SleightOfHand,
    Stealth,
    Arcana,
    History,
    Investigation,
    Nature,
    Religion,
    AnimalHandling,
    Insight,
    Medicine,
    Perception,
    Survival,
    Deception,
    Intimidation,
    Performance,
    Persuasion,
}

impl Skill {
    pub fn ability(&self) -> Ability {
        match self {
            Skill::Athletics => Ability::Strength,
            Skill::Acrobatics | Skill::SleightOfHand | Skill::Stealth => Ability::Dexterity,
            Skill::Arcana | Skill::History | Skill::Investigation | Skill::Nature | Skill::Religion => Ability::Intelligence,
            Skill::AnimalHandling | Skill::Insight | Skill::Medicine | Skill::Perception | Skill::Survival => Ability::Wisdom,
            Skill::Deception | Skill::Intimidation | Skill::Performance | Skill::Persuasion => Ability::Charisma,
        }
    }

    pub fn all() -> &'static [Skill] {
        &[
            Skill::Athletics,
            Skill::Acrobatics,
            Skill::SleightOfHand,
            Skill::Stealth,
            Skill::Arcana,
            Skill::History,
            Skill::Investigation,
            Skill::Nature,
            Skill::Religion,
            Skill::AnimalHandling,
            Skill::Insight,
            Skill::Medicine,
            Skill::Perception,
            Skill::Survival,
            Skill::Deception,
            Skill::Intimidation,
            Skill::Performance,
            Skill::Persuasion,
        ]
    }
}

impl std::fmt::Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Skill::Athletics => write!(f, "Athletics"),
            Skill::Acrobatics => write!(f, "Acrobatics"),
            Skill::SleightOfHand => write!(f, "Sleight of Hand"),
            Skill::Stealth => write!(f, "Stealth"),
            Skill::Arcana => write!(f, "Arcana"),
            Skill::History => write!(f, "History"),
            Skill::Investigation => write!(f, "Investigation"),
            Skill::Nature => write!(f, "Nature"),
            Skill::Religion => write!(f, "Religion"),
            Skill::AnimalHandling => write!(f, "Animal Handling"),
            Skill::Insight => write!(f, "Insight"),
            Skill::Medicine => write!(f, "Medicine"),
            Skill::Perception => write!(f, "Perception"),
            Skill::Survival => write!(f, "Survival"),
            Skill::Deception => write!(f, "Deception"),
            Skill::Intimidation => write!(f, "Intimidation"),
            Skill::Performance => write!(f, "Performance"),
            Skill::Persuasion => write!(f, "Persuasion"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::North => write!(f, "north"),
            Direction::South => write!(f, "south"),
            Direction::East => write!(f, "east"),
            Direction::West => write!(f, "west"),
            Direction::Up => write!(f, "up"),
            Direction::Down => write!(f, "down"),
        }
    }
}

pub type LocationId = u32;
pub type NpcId = u32;
pub type ItemId = u32;
pub type TriggerId = u32;

/// Weapon Mastery property per 2024 SRD. Every weapon has exactly one
/// mastery. The mastery is static data on the weapon; whether a character
/// can *use* it depends on unlocked mastery slots (see
/// `docs/specs/weapon-mastery.md`). Defined here in `types.rs` because
/// `combat`, `equipment`, and `character` all reference it and feature
/// modules cannot depend on each other directly (see the module-isolation
/// decision).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mastery {
    Cleave,
    Graze,
    Nick,
    Push,
    Sap,
    Slow,
    Topple,
    Vex,
}

impl std::fmt::Display for Mastery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mastery::Cleave => write!(f, "Cleave"),
            Mastery::Graze => write!(f, "Graze"),
            Mastery::Nick => write!(f, "Nick"),
            Mastery::Push => write!(f, "Push"),
            Mastery::Sap => write!(f, "Sap"),
            Mastery::Slow => write!(f, "Slow"),
            Mastery::Topple => write!(f, "Topple"),
            Mastery::Vex => write!(f, "Vex"),
        }
    }
}

/// SRD 2024 cover levels. A creature behind cover gains a bonus to AC and
/// Dexterity saving throws based on how much of its body is obscured.
/// See `docs/specs/cover-rules.md` and SRD "Cover" (Playing the Game).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub enum Cover {
    /// No cover — no bonus.
    #[default]
    None,
    /// Half cover — a low wall, large furniture, a creature, etc.
    /// Grants +2 to AC and DEX saving throws.
    Half,
    /// Three-quarters cover — a portcullis, thick tree trunk, etc.
    /// Grants +5 to AC and DEX saving throws.
    ThreeQuarters,
    /// Total cover — completely concealed behind an obstacle.
    /// The creature cannot be directly targeted.
    Total,
}

impl Cover {
    /// Bonus added to the target's AC when this cover level applies.
    pub fn ac_bonus(&self) -> i32 {
        match self {
            Cover::None => 0,
            Cover::Half => 2,
            Cover::ThreeQuarters => 5,
            Cover::Total => 0, // total cover blocks targeting outright
        }
    }

    /// Bonus added to the target's Dexterity saving throw when this cover level applies.
    pub fn save_bonus(&self) -> i32 {
        self.ac_bonus()
    }
}

/// Tool proficiency categories per SRD 2024. Defined in `types.rs` because
/// `character`, `equipment`, and `rules` all reference it and feature modules
/// cannot depend on each other directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolProficiency {
    ThievesTools,
    ArtisansTools,
    GamingSets,
    MusicalInstrument,
    Vehicles,
    NavigatorsTools,
    HerbalismKit,
    HealersKit,
    DisguiseKit,
    ForgeryKit,
    PoisonersKit,
}

impl ToolProficiency {
    /// Return the canonical display name (matches SRD naming convention).
    pub fn name(&self) -> &'static str {
        match self {
            ToolProficiency::ThievesTools => "Thieves' Tools",
            ToolProficiency::ArtisansTools => "Artisan's Tools",
            ToolProficiency::GamingSets => "Gaming Sets",
            ToolProficiency::MusicalInstrument => "Musical Instrument",
            ToolProficiency::Vehicles => "Vehicles",
            ToolProficiency::NavigatorsTools => "Navigator's Tools",
            ToolProficiency::HerbalismKit => "Herbalism Kit",
            ToolProficiency::HealersKit => "Healer's Kit",
            ToolProficiency::DisguiseKit => "Disguise Kit",
            ToolProficiency::ForgeryKit => "Forgery Kit",
            ToolProficiency::PoisonersKit => "Poisoner's Kit",
        }
    }

    /// The ability used for checks with this tool (SRD defaults).
    pub fn check_ability(&self) -> Ability {
        match self {
            ToolProficiency::ThievesTools => Ability::Dexterity,
            ToolProficiency::ArtisansTools => Ability::Strength,
            ToolProficiency::GamingSets => Ability::Intelligence,
            ToolProficiency::MusicalInstrument => Ability::Charisma,
            ToolProficiency::Vehicles => Ability::Dexterity,
            ToolProficiency::NavigatorsTools => Ability::Wisdom,
            ToolProficiency::HerbalismKit => Ability::Wisdom,
            ToolProficiency::HealersKit => Ability::Wisdom,
            ToolProficiency::DisguiseKit => Ability::Charisma,
            ToolProficiency::ForgeryKit => Ability::Dexterity,
            ToolProficiency::PoisonersKit => Ability::Intelligence,
        }
    }

    /// Try to parse a tool name (case-insensitive) to a ToolProficiency.
    pub fn from_name(name: &str) -> Option<ToolProficiency> {
        let lower = name.to_lowercase();
        match lower.as_str() {
            "thieves' tools" | "thieves tools" | "thievestools" => Some(ToolProficiency::ThievesTools),
            "artisan's tools" | "artisans tools" | "artisanstools" => Some(ToolProficiency::ArtisansTools),
            "gaming sets" | "gaming set" => Some(ToolProficiency::GamingSets),
            "musical instrument" | "instrument" => Some(ToolProficiency::MusicalInstrument),
            "vehicles" | "vehicle" => Some(ToolProficiency::Vehicles),
            "navigator's tools" | "navigators tools" | "navigatorstools" => Some(ToolProficiency::NavigatorsTools),
            "herbalism kit" => Some(ToolProficiency::HerbalismKit),
            "healer's kit" | "healers kit" | "healerskit" => Some(ToolProficiency::HealersKit),
            "disguise kit" => Some(ToolProficiency::DisguiseKit),
            "forgery kit" => Some(ToolProficiency::ForgeryKit),
            "poisoner's kit" | "poisoners kit" | "poisonerskit" => Some(ToolProficiency::PoisonersKit),
            _ => None,
        }
    }
}

impl std::fmt::Display for ToolProficiency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Nine-axis alignment plus Unaligned. Shared between characters (chosen at
/// creation) and monsters (declared per stat block). Canonical definition
/// lives here in `types.rs` because it is referenced across feature modules
/// (`character`, `combat::monsters`, `state`), per the module-isolation
/// decision (shared types belong in `types.rs` / `state/`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Alignment {
    LawfulGood,
    NeutralGood,
    ChaoticGood,
    LawfulNeutral,
    TrueNeutral,
    ChaoticNeutral,
    LawfulEvil,
    NeutralEvil,
    ChaoticEvil,
    Unaligned,
}

impl Default for Alignment {
    fn default() -> Self { Alignment::Unaligned }
}

impl std::fmt::Display for Alignment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Alignment::LawfulGood => write!(f, "Lawful Good"),
            Alignment::NeutralGood => write!(f, "Neutral Good"),
            Alignment::ChaoticGood => write!(f, "Chaotic Good"),
            Alignment::LawfulNeutral => write!(f, "Lawful Neutral"),
            // SRD labels "TrueNeutral" as simply "Neutral" in prose.
            Alignment::TrueNeutral => write!(f, "Neutral"),
            Alignment::ChaoticNeutral => write!(f, "Chaotic Neutral"),
            Alignment::LawfulEvil => write!(f, "Lawful Evil"),
            Alignment::NeutralEvil => write!(f, "Neutral Evil"),
            Alignment::ChaoticEvil => write!(f, "Chaotic Evil"),
            Alignment::Unaligned => write!(f, "Unaligned"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ability_modifier() {
        assert_eq!(Ability::modifier(10), 0);
        assert_eq!(Ability::modifier(11), 0);
        assert_eq!(Ability::modifier(12), 1);
        assert_eq!(Ability::modifier(8), -1);
        assert_eq!(Ability::modifier(1), -5);
        assert_eq!(Ability::modifier(20), 5);
    }

    #[test]
    fn test_skill_ability_mapping() {
        assert_eq!(Skill::Athletics.ability(), Ability::Strength);
        assert_eq!(Skill::Stealth.ability(), Ability::Dexterity);
        assert_eq!(Skill::Arcana.ability(), Ability::Intelligence);
        assert_eq!(Skill::Perception.ability(), Ability::Wisdom);
        assert_eq!(Skill::Persuasion.ability(), Ability::Charisma);
    }

    #[test]
    fn test_alignment_default_is_unaligned() {
        assert_eq!(Alignment::default(), Alignment::Unaligned);
    }

    #[test]
    fn test_alignment_exhaustive_variants() {
        // Exhaustiveness: the ten canonical alignments (9 + Unaligned).
        let _ = match Alignment::TrueNeutral {
            Alignment::LawfulGood | Alignment::NeutralGood | Alignment::ChaoticGood
            | Alignment::LawfulNeutral | Alignment::TrueNeutral | Alignment::ChaoticNeutral
            | Alignment::LawfulEvil | Alignment::NeutralEvil | Alignment::ChaoticEvil
            | Alignment::Unaligned => (),
        };
    }

    #[test]
    fn test_alignment_serde_round_trip() {
        for al in [
            Alignment::LawfulGood,
            Alignment::NeutralGood,
            Alignment::ChaoticGood,
            Alignment::LawfulNeutral,
            Alignment::TrueNeutral,
            Alignment::ChaoticNeutral,
            Alignment::LawfulEvil,
            Alignment::NeutralEvil,
            Alignment::ChaoticEvil,
            Alignment::Unaligned,
        ] {
            let json = serde_json::to_string(&al).unwrap();
            let back: Alignment = serde_json::from_str(&json).unwrap();
            assert_eq!(back, al);
        }
    }

    #[test]
    fn test_alignment_display() {
        assert_eq!(Alignment::LawfulGood.to_string(), "Lawful Good");
        assert_eq!(Alignment::TrueNeutral.to_string(), "Neutral");
        assert_eq!(Alignment::Unaligned.to_string(), "Unaligned");
    }

    // ---- Weapon Mastery (2024 SRD) ----

    #[test]
    fn test_mastery_all_variants_exist() {
        // All 8 SRD 2024 masteries must exist as enum variants.
        let _ = [
            Mastery::Cleave,
            Mastery::Graze,
            Mastery::Nick,
            Mastery::Push,
            Mastery::Sap,
            Mastery::Slow,
            Mastery::Topple,
            Mastery::Vex,
        ];
    }

    #[test]
    fn test_mastery_display_matches_srd_names() {
        assert_eq!(Mastery::Cleave.to_string(), "Cleave");
        assert_eq!(Mastery::Graze.to_string(), "Graze");
        assert_eq!(Mastery::Nick.to_string(), "Nick");
        assert_eq!(Mastery::Push.to_string(), "Push");
        assert_eq!(Mastery::Sap.to_string(), "Sap");
        assert_eq!(Mastery::Slow.to_string(), "Slow");
        assert_eq!(Mastery::Topple.to_string(), "Topple");
        assert_eq!(Mastery::Vex.to_string(), "Vex");
    }

    #[test]
    fn test_mastery_serde_roundtrip() {
        for m in [
            Mastery::Cleave,
            Mastery::Graze,
            Mastery::Nick,
            Mastery::Push,
            Mastery::Sap,
            Mastery::Slow,
            Mastery::Topple,
            Mastery::Vex,
        ] {
            let json = serde_json::to_string(&m).unwrap();
            let back: Mastery = serde_json::from_str(&json).unwrap();
            assert_eq!(back, m);
        }
    }
}
