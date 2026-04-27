// jurnalis-engine/src/character/feat.rs
//
// SRD 2024 feat catalog. Compile-time const table in the same style as
// `equipment::SRD_WEAPONS` / `SRD_ARMOR` (see
// `docs/decisions/srd-const-tables.md`).
//
// This module MUST NOT import from `combat/`, `equipment/`, `spells/`, or
// `narration/` (see `docs/decisions/module-isolation.md`). Applying feat
// effects at the character / world level is the orchestrator's job (in
// `lib.rs`). The data here is pure — each feat is a name + description +
// category + list of static effects.

use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};

/// Category of a feat, used for prerequisite gating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatCategory {
    /// Taken at character creation; no prerequisite beyond "one per character".
    Origin,
    /// Taken at level 4+ in place of an ASI.
    General,
    /// Class-gated (Fighter / Paladin / Ranger only in MVP).
    FightingStyle,
}

/// A single mechanical effect of a feat. Multiple effects combine on a feat.
///
/// `Flavor` marks feats whose mechanical hooks haven't landed yet (combat
/// integration, prerequisite checking, etc.). The feat is still selectable
/// and visible on the sheet; its full behavior lands in follow-up features.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeatEffect {
    /// Flat bonus to an ability score (capped at 20 at application time).
    AbilityBonus { ability: Ability, amount: i32 },
    /// Grants proficiency in a specific skill.
    SkillProficiency(Skill),
    /// Flat bonus added to the initiative roll.
    Initiative(i32),
    /// Flat bonus to max HP, scaled by character level (Tough: +2/level).
    HpBonusPerLevel(i32),
    /// Placeholder for "gain one language of your choice" — the MVP records
    /// the feat but does not prompt for a specific language yet.
    LanguageProficiency,
    /// Placeholder for "gain one tool of your choice".
    ToolProficiency,
    /// Flat bonus to movement speed.
    SpeedBonus(i32),
    /// Feat whose mechanical side-effects are deferred to future features.
    Flavor,
}

/// Static definition of a feat.
pub struct FeatDef {
    pub name: &'static str,
    pub description: &'static str,
    pub category: FeatCategory,
    pub effects: &'static [FeatEffect],
}

impl FeatDef {
    /// Case-insensitive lookup by feat name.
    pub fn lookup(name: &str) -> Option<&'static FeatDef> {
        let needle = name.trim().to_lowercase();
        FEATS.iter().find(|f| f.name.to_lowercase() == needle)
    }
}

/// SRD 2024 feat catalog (origin + general + fighting-style).
///
/// Effects marked `Flavor` are selectable but their mechanical hooks (combat
/// damage toggles, reaction attacks, concentration advantage, etc.) land in
/// combat/spell-integration follow-ups. See `docs/specs/feat-system.md`.
pub const FEATS: &[FeatDef] = &[
    // ---- Origin feats (9) ----
    FeatDef {
        name: "Alert",
        description: "You gain a +5 bonus to initiative rolls.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Initiative(5)],
    },
    FeatDef {
        name: "Crafter",
        description: "You gain proficiency with one tool and a 20% discount on non-magical crafted gear.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::ToolProficiency],
    },
    FeatDef {
        name: "Healer",
        description: "You can stabilize a dying creature as an action and restore 1d4+4 HP once per short rest.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Lucky",
        description: "You have 3 luck points per day. Spend one to reroll an attack, ability check, or save.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Magic Initiate",
        description: "Learn two cantrips and a level-1 spell from a chosen class's spell list.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Musician",
        description: "You have 3 uses of Bardic Inspiration per long rest.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Savage Attacker",
        description: "Once per turn on a weapon hit, reroll the weapon's damage dice and take the higher result.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Skilled",
        description: "Gain proficiency in any combination of three skills or tools.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Tavern Brawler",
        description: "Your unarmed strike damage is 1d4 + STR. You can grapple on a hit.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::Flavor],
    },

    // ---- General feats (7) ----
    FeatDef {
        name: "Ability Score Improvement",
        description: "Increase one ability score by 2 or two ability scores by 1 (max 20).",
        category: FeatCategory::General,
        // Applied directly by the ChooseAsi dispatcher; no static effect.
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Grappler",
        description: "Advantage on grapple attempts; bonus to STR or DEX.",
        category: FeatCategory::General,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Great Weapon Master",
        description: "Before an attack with a heavy weapon, take -5 to attack for +10 damage.",
        category: FeatCategory::General,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Sharpshooter",
        description: "Ignore half/three-quarters cover; -5 attack for +10 damage with ranged weapons.",
        category: FeatCategory::General,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Sentinel",
        description: "Opportunity attacks stop movement; reaction attack when adjacent ally is attacked.",
        category: FeatCategory::General,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Tough",
        description: "Your hit point maximum increases by 2 per character level.",
        category: FeatCategory::Origin,
        effects: &[FeatEffect::HpBonusPerLevel(2)],
    },
    FeatDef {
        name: "War Caster",
        description: "Advantage on concentration saves; can cast spells with full hands.",
        category: FeatCategory::General,
        effects: &[FeatEffect::Flavor],
    },

    // ---- Fighting Style feats (6) ----
    FeatDef {
        name: "Archery",
        description: "+2 bonus to attack rolls with ranged weapons.",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Defense",
        description: "+1 AC while wearing armor.",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Dueling",
        description: "+2 damage with a one-handed melee weapon and no other weapon.",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Great Weapon Fighting",
        description: "Reroll 1s and 2s on melee damage dice with two-handed weapons.",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Protection",
        description: "Impose disadvantage on attack against an adjacent ally (reaction).",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
    FeatDef {
        name: "Two-Weapon Fighting",
        description: "Add your ability modifier to off-hand attack damage.",
        category: FeatCategory::FightingStyle,
        effects: &[FeatEffect::Flavor],
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feat_catalog_has_expected_count() {
        // 10 origin + 6 general + 6 fighting-style = 22
        assert_eq!(FEATS.len(), 22);
    }

    #[test]
    fn test_lookup_case_insensitive() {
        assert!(FeatDef::lookup("alert").is_some());
        assert!(FeatDef::lookup("ALERT").is_some());
        assert!(FeatDef::lookup("Alert").is_some());
        assert!(FeatDef::lookup("  alert  ").is_some());
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(FeatDef::lookup("nonsense").is_none());
        assert!(FeatDef::lookup("").is_none());
    }

    #[test]
    fn test_all_feats_have_nonempty_name_and_description() {
        for f in FEATS {
            assert!(!f.name.is_empty(), "feat has empty name");
            assert!(!f.description.is_empty(), "feat {} has empty description", f.name);
        }
    }

    #[test]
    fn test_alert_has_initiative_bonus_of_5() {
        let alert = FeatDef::lookup("Alert").expect("Alert exists");
        let has_init = alert.effects.iter().any(|e| matches!(e, FeatEffect::Initiative(5)));
        assert!(has_init, "Alert must have Initiative(5) effect");
    }

    #[test]
    fn test_tough_has_hp_bonus_per_level_of_2() {
        let tough = FeatDef::lookup("Tough").expect("Tough exists");
        let has_hp = tough.effects.iter().any(|e| matches!(e, FeatEffect::HpBonusPerLevel(2)));
        assert!(has_hp, "Tough must have HpBonusPerLevel(2) effect");
    }

    #[test]
    fn test_origin_feats_have_origin_category() {
        for name in ["Alert", "Crafter", "Healer", "Lucky", "Magic Initiate",
                     "Musician", "Savage Attacker", "Skilled", "Tavern Brawler", "Tough"] {
            let f = FeatDef::lookup(name).unwrap_or_else(|| panic!("{} not found", name));
            assert_eq!(f.category, FeatCategory::Origin, "{} should be Origin", name);
        }
    }

    #[test]
    fn test_general_feats_have_general_category() {
        for name in ["Ability Score Improvement", "Grappler", "Great Weapon Master",
                     "Sharpshooter", "Sentinel", "War Caster"] {
            let f = FeatDef::lookup(name).unwrap_or_else(|| panic!("{} not found", name));
            assert_eq!(f.category, FeatCategory::General, "{} should be General", name);
        }
    }

    #[test]
    fn test_fighting_style_feats_have_fighting_style_category() {
        for name in ["Archery", "Defense", "Dueling", "Great Weapon Fighting",
                     "Protection", "Two-Weapon Fighting"] {
            let f = FeatDef::lookup(name).unwrap_or_else(|| panic!("{} not found", name));
            assert_eq!(f.category, FeatCategory::FightingStyle, "{} should be FightingStyle", name);
        }
    }
}
