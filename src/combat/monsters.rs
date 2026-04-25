// jurnalis-engine/src/combat/monsters.rs
// SRD monster const table for combat encounters.

use std::collections::HashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::{Ability, Alignment};
use crate::state::{CombatStats, NpcAttack, DamageType};
use crate::conditions::ConditionType;

/// SRD creature type. See `docs/reference/monsters.md` for the canonical list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CreatureType {
    Aberration,
    Beast,
    Celestial,
    Construct,
    Dragon,
    Elemental,
    Fey,
    Fiend,
    Giant,
    Humanoid,
    Monstrosity,
    Ooze,
    Plant,
    Undead,
}

impl Default for CreatureType {
    fn default() -> Self { CreatureType::Humanoid }
}

/// SRD creature size. Determines hit-die size in the SRD (informational here).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Size {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
    Gargantuan,
}

impl Default for Size {
    fn default() -> Self { Size::Medium }
}

/// Default value for `CombatStats.multiattack` when absent from older saves.
pub fn default_multiattack() -> u32 { 1 }

/// Static monster definition for the const table.
pub struct MonsterDef {
    pub name: &'static str,
    pub max_hp: i32,
    pub ac: i32,
    pub speed: i32,
    pub str_: i32,
    pub dex: i32,
    pub con: i32,
    pub int: i32,
    pub wis: i32,
    pub cha: i32,
    pub proficiency_bonus: i32,
    pub attacks: &'static [MonsterAttackDef],
    /// SRD challenge rating. Drives XP awarded on defeat (see `leveling::xp_for_cr`).
    /// Fractional values match SRD: 0, 1/8 = 0.125, 1/4 = 0.25, 1/2 = 0.5, etc.
    pub cr: f32,
    /// SRD creature type (Beast, Undead, Humanoid, etc.).
    pub creature_type: CreatureType,
    /// SRD size category (Tiny..Gargantuan).
    pub size: Size,
    /// Default alignment per the SRD stat block.
    pub alignment: Alignment,
    /// Damage types this monster takes half damage from.
    pub damage_resistances: &'static [DamageType],
    /// Damage types this monster ignores entirely.
    pub damage_immunities: &'static [DamageType],
    /// Condition types this monster cannot be afflicted by.
    pub condition_immunities: &'static [ConditionType],
    /// Free-form sense descriptors (e.g. "Darkvision 60 ft.", "Passive Perception 10").
    pub senses: &'static [&'static str],
    /// Languages spoken or understood. Empty slice means none.
    pub languages: &'static [&'static str],
    /// Number of attacks per Attack action. 1 = single attack (no multiattack).
    pub multiattack: u32,
    /// Free-form special traits surfaced to narration. `(name, description)` pairs.
    /// No mechanical effect in the MVP; informational only.
    pub special_traits: &'static [(&'static str, &'static str)],
}

pub struct MonsterAttackDef {
    pub name: &'static str,
    pub hit_bonus: i32,
    pub damage_dice: u32,
    pub damage_die: u32,
    pub damage_bonus: i32,
    pub damage_type: DamageType,
    pub reach: u32,
    pub range_normal: u32,
    pub range_long: u32,
}

// Reusable empty slices for monsters with no entries in a given column.
const NO_DAMAGE_TYPES: &[DamageType] = &[];
const NO_CONDITIONS: &[ConditionType] = &[];
const NO_TRAITS: &[(&str, &str)] = &[];

// SRD monster table. The first 12 entries are the original "core" set with
// canonical CRs from `docs/specs/leveling-and-xp.md`; entries beyond that
// were added for the monster-stat-blocks feature.
pub const SRD_MONSTERS: &[MonsterDef] = &[
    MonsterDef {
        name: "Rat", max_hp: 1, ac: 10, speed: 20,
        str_: 2, dex: 11, con: 9, int: 2, wis: 10, cha: 4,
        proficiency_bonus: 2,
        cr: 0.0,
        attacks: &[MonsterAttackDef {
            name: "Bite", hit_bonus: 0, damage_dice: 1, damage_die: 1, damage_bonus: 0,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Tiny,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 30 ft.", "Passive Perception 12"],
        languages: &[],
        multiattack: 1,
        special_traits: &[
            ("Agile", "The rat doesn't provoke an Opportunity Attack when it moves out of an enemy's reach."),
        ],
    },
    MonsterDef {
        name: "Kobold", max_hp: 5, ac: 12, speed: 30,
        str_: 7, dex: 15, con: 9, int: 8, wis: 7, cha: 8,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[MonsterAttackDef {
            name: "Dagger", hit_bonus: 4, damage_dice: 1, damage_die: 4, damage_bonus: 2,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 20, range_long: 60,
        }],
        creature_type: CreatureType::Dragon,
        size: Size::Small,
        alignment: Alignment::TrueNeutral,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 8"],
        languages: &["Common", "Draconic"],
        multiattack: 1,
        special_traits: &[
            ("Pack Tactics", "Advantage on attack rolls vs a creature if at least one ally is within 5 ft and not Incapacitated."),
            ("Sunlight Sensitivity", "Disadvantage on attack rolls and ability checks while in sunlight."),
        ],
    },
    MonsterDef {
        name: "Goblin", max_hp: 7, ac: 15, speed: 30,
        str_: 8, dex: 14, con: 10, int: 10, wis: 8, cha: 8,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[
            MonsterAttackDef {
                name: "Scimitar", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Shortbow", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
        creature_type: CreatureType::Fey,
        size: Size::Small,
        alignment: Alignment::ChaoticNeutral,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 9"],
        languages: &["Common", "Goblin"],
        multiattack: 1,
        special_traits: &[
            ("Nimble Escape", "The goblin can take the Disengage or Hide action as a Bonus Action."),
        ],
    },
    MonsterDef {
        name: "Skeleton", max_hp: 13, ac: 13, speed: 30,
        str_: 10, dex: 14, con: 15, int: 6, wis: 8, cha: 5,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[
            MonsterAttackDef {
                name: "Shortsword", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Shortbow", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
        creature_type: CreatureType::Undead,
        size: Size::Medium,
        alignment: Alignment::LawfulEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: &[DamageType::Poison],
        condition_immunities: &[ConditionType::Exhaustion, ConditionType::Poisoned],
        senses: &["Darkvision 60 ft.", "Passive Perception 9"],
        languages: &["Common"],
        multiattack: 1,
        special_traits: NO_TRAITS,
    },
    MonsterDef {
        name: "Zombie", max_hp: 22, ac: 8, speed: 20,
        str_: 13, dex: 6, con: 16, int: 3, wis: 6, cha: 5,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[MonsterAttackDef {
            name: "Slam", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
            damage_type: DamageType::Bludgeoning, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Undead,
        size: Size::Medium,
        alignment: Alignment::NeutralEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: &[DamageType::Poison],
        condition_immunities: &[ConditionType::Exhaustion, ConditionType::Poisoned],
        senses: &["Darkvision 60 ft.", "Passive Perception 8"],
        languages: &["Common"],
        multiattack: 1,
        special_traits: &[
            ("Undead Fortitude", "If damage reduces the zombie to 0 HP, it makes a CON save (DC 5 + damage taken) unless the damage is Radiant or from a Critical Hit. On a success, it drops to 1 HP instead."),
        ],
    },
    MonsterDef {
        name: "Guard", max_hp: 11, ac: 16, speed: 30,
        str_: 13, dex: 12, con: 12, int: 10, wis: 11, cha: 10,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[MonsterAttackDef {
            name: "Spear", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 20, range_long: 60,
        }],
        creature_type: CreatureType::Humanoid,
        size: Size::Medium,
        alignment: Alignment::TrueNeutral,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Passive Perception 12"],
        languages: &["Common"],
        multiattack: 1,
        special_traits: NO_TRAITS,
    },
    MonsterDef {
        name: "Bandit", max_hp: 11, ac: 12, speed: 30,
        str_: 11, dex: 12, con: 12, int: 10, wis: 10, cha: 10,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[
            MonsterAttackDef {
                name: "Scimitar", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Light Crossbow", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
        creature_type: CreatureType::Humanoid,
        size: Size::Medium,
        alignment: Alignment::TrueNeutral,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Passive Perception 10"],
        languages: &["Common"],
        multiattack: 1,
        special_traits: NO_TRAITS,
    },
    MonsterDef {
        name: "Orc", max_hp: 15, ac: 13, speed: 30,
        str_: 16, dex: 12, con: 16, int: 7, wis: 11, cha: 10,
        proficiency_bonus: 2,
        cr: 0.5,
        attacks: &[
            MonsterAttackDef {
                name: "Greataxe", hit_bonus: 5, damage_dice: 1, damage_die: 12, damage_bonus: 3,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 5, damage_dice: 1, damage_die: 6, damage_bonus: 3,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
        creature_type: CreatureType::Humanoid,
        size: Size::Medium,
        alignment: Alignment::ChaoticEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 10"],
        languages: &["Common", "Orc"],
        multiattack: 1,
        special_traits: &[
            ("Aggressive", "As a Bonus Action, the orc moves up to its Speed toward a hostile creature it can see."),
        ],
    },
    MonsterDef {
        name: "Hobgoblin", max_hp: 11, ac: 18, speed: 30,
        str_: 13, dex: 12, con: 12, int: 10, wis: 10, cha: 9,
        proficiency_bonus: 2,
        cr: 0.5,
        attacks: &[
            MonsterAttackDef {
                name: "Longsword", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Longbow", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 150, range_long: 600,
            },
        ],
        creature_type: CreatureType::Fey,
        size: Size::Medium,
        alignment: Alignment::LawfulEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 10"],
        languages: &["Common", "Goblin"],
        multiattack: 1,
        special_traits: &[
            ("Pack Tactics", "Advantage on attack rolls vs a creature if at least one ally is within 5 ft and not Incapacitated."),
        ],
    },
    MonsterDef {
        name: "Bugbear", max_hp: 27, ac: 16, speed: 30,
        str_: 15, dex: 14, con: 13, int: 8, wis: 11, cha: 9,
        proficiency_bonus: 2,
        cr: 1.0,
        attacks: &[
            MonsterAttackDef {
                name: "Morningstar", hit_bonus: 4, damage_dice: 2, damage_die: 8, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 4, damage_dice: 2, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
        creature_type: CreatureType::Fey,
        size: Size::Medium,
        alignment: Alignment::ChaoticEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 10"],
        languages: &["Common", "Goblin"],
        multiattack: 2,
        special_traits: &[
            ("Brute", "A melee weapon deals one extra die of damage when the bugbear hits with it (already included in damage)."),
        ],
    },
    MonsterDef {
        name: "Ghoul", max_hp: 22, ac: 12, speed: 30,
        str_: 13, dex: 15, con: 10, int: 7, wis: 10, cha: 6,
        proficiency_bonus: 2,
        cr: 1.0,
        attacks: &[
            MonsterAttackDef {
                name: "Claws", hit_bonus: 4, damage_dice: 2, damage_die: 4, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Bite", hit_bonus: 2, damage_dice: 2, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
        ],
        creature_type: CreatureType::Undead,
        size: Size::Medium,
        alignment: Alignment::ChaoticEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: &[DamageType::Poison],
        condition_immunities: &[
            ConditionType::Charmed,
            ConditionType::Exhaustion,
            ConditionType::Poisoned,
        ],
        senses: &["Darkvision 60 ft.", "Passive Perception 10"],
        languages: &["Common"],
        multiattack: 2,
        special_traits: NO_TRAITS,
    },
    MonsterDef {
        name: "Ogre", max_hp: 59, ac: 11, speed: 40,
        str_: 19, dex: 8, con: 16, int: 5, wis: 7, cha: 7,
        proficiency_bonus: 2,
        cr: 2.0,
        attacks: &[
            MonsterAttackDef {
                name: "Greatclub", hit_bonus: 6, damage_dice: 2, damage_die: 8, damage_bonus: 4,
                damage_type: DamageType::Bludgeoning, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 6, damage_dice: 2, damage_die: 6, damage_bonus: 4,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
        creature_type: CreatureType::Giant,
        size: Size::Large,
        alignment: Alignment::ChaoticEvil,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 8"],
        languages: &["Common", "Giant"],
        multiattack: 1,
        special_traits: NO_TRAITS,
    },
    // ---- New entries (monster-stat-blocks feature, 2026-04-15) ----
    MonsterDef {
        name: "Wolf", max_hp: 11, ac: 12, speed: 40,
        str_: 14, dex: 15, con: 12, int: 3, wis: 12, cha: 6,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[MonsterAttackDef {
            name: "Bite", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Medium,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 15"],
        languages: &[],
        multiattack: 1,
        special_traits: &[
            ("Pack Tactics", "Advantage on attack rolls vs a creature if at least one ally is within 5 ft and not Incapacitated."),
        ],
    },
    MonsterDef {
        name: "Bat", max_hp: 1, ac: 12, speed: 5,
        str_: 2, dex: 15, con: 8, int: 2, wis: 12, cha: 4,
        proficiency_bonus: 2,
        cr: 0.0,
        attacks: &[MonsterAttackDef {
            name: "Bite", hit_bonus: 4, damage_dice: 1, damage_die: 1, damage_bonus: 0,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Tiny,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Blindsight 60 ft.", "Passive Perception 11"],
        languages: &[],
        multiattack: 1,
        special_traits: &[
            ("Echolocation", "While unable to hear, the bat has no Blindsight."),
        ],
    },
    MonsterDef {
        name: "Spider", max_hp: 1, ac: 12, speed: 20,
        str_: 2, dex: 14, con: 8, int: 1, wis: 10, cha: 2,
        proficiency_bonus: 2,
        cr: 0.0,
        attacks: &[MonsterAttackDef {
            // Bite: 1 piercing + 2 (1d4) poison; we model the primary attack
            // as the piercing portion; poison rider is informational in the trait list.
            name: "Bite", hit_bonus: 4, damage_dice: 1, damage_die: 1, damage_bonus: 0,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Tiny,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 30 ft.", "Passive Perception 10"],
        languages: &[],
        multiattack: 1,
        special_traits: &[
            ("Spider Climb", "Can climb difficult surfaces, including ceilings, without an ability check."),
            ("Web Walker", "Ignores movement restrictions caused by webs."),
        ],
    },
    MonsterDef {
        name: "Boar", max_hp: 11, ac: 11, speed: 40,
        str_: 13, dex: 11, con: 12, int: 2, wis: 9, cha: 5,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[MonsterAttackDef {
            name: "Tusk", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
            damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Medium,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Passive Perception 9"],
        languages: &[],
        multiattack: 1,
        special_traits: &[
            ("Charge", "If the boar moves 20+ feet straight toward a target and hits with a Tusk on the same turn, the target takes an extra 3 (1d6) Slashing damage."),
            ("Relentless", "If the boar takes 7 damage or less that would reduce it to 0 HP, it is reduced to 1 HP instead. (Recharges after a Short or Long Rest.)"),
        ],
    },
    MonsterDef {
        name: "Black Bear", max_hp: 19, ac: 11, speed: 30,
        str_: 15, dex: 12, con: 14, int: 2, wis: 12, cha: 7,
        proficiency_bonus: 2,
        cr: 0.5,
        attacks: &[MonsterAttackDef {
            name: "Rend", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
            damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
        }],
        creature_type: CreatureType::Beast,
        size: Size::Medium,
        alignment: Alignment::Unaligned,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 15"],
        languages: &[],
        multiattack: 2,
        special_traits: NO_TRAITS,
    },
    MonsterDef {
        name: "Goblin Boss", max_hp: 21, ac: 17, speed: 30,
        str_: 10, dex: 15, con: 10, int: 10, wis: 8, cha: 10,
        proficiency_bonus: 2,
        cr: 1.0,
        attacks: &[
            MonsterAttackDef {
                name: "Scimitar", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Shortbow", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
        creature_type: CreatureType::Fey,
        size: Size::Small,
        alignment: Alignment::ChaoticNeutral,
        damage_resistances: NO_DAMAGE_TYPES,
        damage_immunities: NO_DAMAGE_TYPES,
        condition_immunities: NO_CONDITIONS,
        senses: &["Darkvision 60 ft.", "Passive Perception 9"],
        languages: &["Common", "Goblin"],
        multiattack: 2,
        special_traits: &[
            ("Nimble Escape", "Can take the Disengage or Hide action as a Bonus Action."),
            ("Redirect Attack", "Reaction: when an attack would hit the boss, swap places with a Small/Medium ally within 5 ft and the ally becomes the target instead."),
        ],
    },
];

/// Look up a monster definition by name (case-insensitive).
pub fn find_monster(name: &str) -> Option<&'static MonsterDef> {
    let lower = name.to_lowercase();
    SRD_MONSTERS.iter().find(|m| m.name.to_lowercase() == lower)
}

/// Convert a MonsterDef into a CombatStats instance.
pub fn monster_to_combat_stats(def: &MonsterDef) -> CombatStats {
    let mut ability_scores = HashMap::new();
    ability_scores.insert(Ability::Strength, def.str_);
    ability_scores.insert(Ability::Dexterity, def.dex);
    ability_scores.insert(Ability::Constitution, def.con);
    ability_scores.insert(Ability::Intelligence, def.int);
    ability_scores.insert(Ability::Wisdom, def.wis);
    ability_scores.insert(Ability::Charisma, def.cha);

    let attacks = def.attacks.iter().map(|a| NpcAttack {
        name: a.name.to_string(),
        hit_bonus: a.hit_bonus,
        damage_dice: a.damage_dice,
        damage_die: a.damage_die,
        damage_bonus: a.damage_bonus,
        damage_type: a.damage_type,
        reach: a.reach,
        range_normal: a.range_normal,
        range_long: a.range_long,
    }).collect();

    CombatStats {
        max_hp: def.max_hp,
        current_hp: def.max_hp,
        ac: def.ac,
        speed: def.speed,
        ability_scores,
        attacks,
        proficiency_bonus: def.proficiency_bonus,
        cr: def.cr,
        creature_type: def.creature_type,
        size: def.size,
        alignment: def.alignment,
        damage_resistances: def.damage_resistances.to_vec(),
        damage_immunities: def.damage_immunities.to_vec(),
        condition_immunities: def.condition_immunities.to_vec(),
        senses: def.senses.iter().map(|s| s.to_string()).collect(),
        languages: def.languages.iter().map(|s| s.to_string()).collect(),
        multiattack: def.multiattack,
        special_traits: def.special_traits.iter()
            .map(|(n, d)| (n.to_string(), d.to_string()))
            .collect(),
        spells: Vec::new(),
    }
}

/// HP target window for a given depth tier. Depth is a location index proxy
/// (0 = entrance, increasing with distance from spawn). See
/// `docs/specs/world-generation.md` for the authoritative definition.
fn hp_window_for_depth(depth: usize) -> (i32, i32) {
    match depth {
        0..=3 => (5, 12),
        4..=8 => (10, 18),
        _ => (15, 25),
    }
}

/// Pick an `SRD_MONSTERS` entry whose `max_hp` falls within the depth's
/// target window. If no monster matches the window (defensive fallback),
/// return the monster whose `max_hp` is closest to the window's midpoint,
/// with ties broken by table order.
///
/// This biases early rooms (low depth) toward weaker foes, scaling up with
/// distance from the player's spawn. Selection is deterministic for a given
/// RNG state, preserving world-generation reproducibility.
pub fn select_monster_for_depth(rng: &mut impl Rng, depth: usize) -> &'static MonsterDef {
    let (lo, hi) = hp_window_for_depth(depth);

    let matching: Vec<&'static MonsterDef> = SRD_MONSTERS
        .iter()
        .filter(|m| m.max_hp >= lo && m.max_hp <= hi)
        .collect();

    if !matching.is_empty() {
        let idx = rng.gen_range(0..matching.len());
        return matching[idx];
    }

    // Defensive fallback: pick the monster closest to the window midpoint.
    // Ties are broken by table order (first occurrence wins).
    let mid = (lo + hi) / 2;
    SRD_MONSTERS
        .iter()
        .min_by_key(|m| (m.max_hp - mid).abs())
        .expect("SRD_MONSTERS must not be empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_srd_monsters_count() {
        // 12 original entries (per leveling-and-xp spec) plus stat-block additions.
        assert!(SRD_MONSTERS.len() >= 18,
            "expected at least 18 monsters (12 core + new entries), got {}",
            SRD_MONSTERS.len());
    }

    #[test]
    fn test_core_twelve_monsters_present() {
        for name in &[
            "Rat", "Kobold", "Goblin", "Skeleton", "Zombie", "Guard",
            "Bandit", "Orc", "Hobgoblin", "Bugbear", "Ghoul", "Ogre",
        ] {
            assert!(find_monster(name).is_some(),
                "core monster '{}' missing from SRD_MONSTERS", name);
        }
    }

    #[test]
    fn test_new_monsters_present() {
        for name in &["Wolf", "Bat", "Spider", "Boar", "Black Bear", "Goblin Boss"] {
            assert!(find_monster(name).is_some(),
                "new monster '{}' missing from SRD_MONSTERS", name);
        }
    }

    #[test]
    fn test_select_monster_for_depth_tier_0_hp_range() {
        // Depth 0-3 should bias toward HP 5-12.
        for depth in 0..=3 {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 5 && def.max_hp <= 12,
                    "depth {} seed {}: picked {} with HP {}, expected 5-12",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_tier_1_hp_range() {
        // Depth 4-8 should bias toward HP 10-18.
        for depth in 4..=8 {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 10 && def.max_hp <= 18,
                    "depth {} seed {}: picked {} with HP {}, expected 10-18",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_tier_2_hp_range() {
        // Depth 9+ should bias toward HP 15-25.
        for depth in [9usize, 12, 20, 100] {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 15 && def.max_hp <= 25,
                    "depth {} seed {}: picked {} with HP {}, expected 15-25",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(7);
        let mut rng2 = StdRng::seed_from_u64(7);
        let a = select_monster_for_depth(&mut rng1, 2);
        let b = select_monster_for_depth(&mut rng2, 2);
        assert_eq!(a.name, b.name);
        assert_eq!(a.max_hp, b.max_hp);
    }

    #[test]
    fn test_find_monster_by_name() {
        assert!(find_monster("Goblin").is_some());
        assert!(find_monster("goblin").is_some());
        assert!(find_monster("nonexistent").is_none());
    }

    #[test]
    fn test_goblin_stats() {
        let goblin = find_monster("Goblin").unwrap();
        assert_eq!(goblin.max_hp, 7);
        assert_eq!(goblin.ac, 15);
        assert_eq!(goblin.speed, 30);
        assert_eq!(goblin.attacks.len(), 2);
    }

    #[test]
    fn test_ogre_stats() {
        let ogre = find_monster("Ogre").unwrap();
        assert_eq!(ogre.max_hp, 59);
        assert_eq!(ogre.ac, 11);
        assert_eq!(ogre.str_, 19);
    }

    #[test]
    fn test_monster_to_combat_stats() {
        let goblin = find_monster("Goblin").unwrap();
        let stats = monster_to_combat_stats(goblin);
        assert_eq!(stats.max_hp, 7);
        assert_eq!(stats.current_hp, 7);
        assert_eq!(stats.ac, 15);
        assert_eq!(stats.speed, 30);
        assert_eq!(stats.attacks.len(), 2);
        assert_eq!(stats.attacks[0].name, "Scimitar");
        assert_eq!(*stats.ability_scores.get(&Ability::Dexterity).unwrap(), 14);
    }

    #[test]
    fn test_all_monsters_have_attacks() {
        for monster in SRD_MONSTERS {
            assert!(!monster.attacks.is_empty(), "{} has no attacks", monster.name);
        }
    }

    #[test]
    fn test_all_monsters_positive_hp() {
        for monster in SRD_MONSTERS {
            assert!(monster.max_hp > 0, "{} has non-positive HP", monster.name);
        }
    }

    #[test]
    fn test_all_monsters_have_finite_nonneg_cr() {
        for monster in SRD_MONSTERS {
            assert!(monster.cr.is_finite(), "{} has non-finite CR", monster.name);
            assert!(monster.cr >= 0.0, "{} has negative CR", monster.name);
        }
    }

    #[test]
    fn test_creature_type_has_all_srd_variants() {
        // Compile-time check via match exhaustion: every SRD creature type listed.
        let v = CreatureType::Beast;
        let _name = match v {
            CreatureType::Aberration => "aberration",
            CreatureType::Beast => "beast",
            CreatureType::Celestial => "celestial",
            CreatureType::Construct => "construct",
            CreatureType::Dragon => "dragon",
            CreatureType::Elemental => "elemental",
            CreatureType::Fey => "fey",
            CreatureType::Fiend => "fiend",
            CreatureType::Giant => "giant",
            CreatureType::Humanoid => "humanoid",
            CreatureType::Monstrosity => "monstrosity",
            CreatureType::Ooze => "ooze",
            CreatureType::Plant => "plant",
            CreatureType::Undead => "undead",
        };
        assert_eq!(CreatureType::default(), CreatureType::Humanoid);
    }

    #[test]
    fn test_size_default_is_medium() {
        assert_eq!(Size::default(), Size::Medium);
        // Exhaustiveness:
        let _ = match Size::Tiny {
            Size::Tiny | Size::Small | Size::Medium |
            Size::Large | Size::Huge | Size::Gargantuan => (),
        };
    }

    #[test]
    fn test_alignment_default_is_unaligned() {
        assert_eq!(Alignment::default(), Alignment::Unaligned);
        let _ = match Alignment::TrueNeutral {
            Alignment::LawfulGood | Alignment::NeutralGood | Alignment::ChaoticGood |
            Alignment::LawfulNeutral | Alignment::TrueNeutral | Alignment::ChaoticNeutral |
            Alignment::LawfulEvil | Alignment::NeutralEvil | Alignment::ChaoticEvil |
            Alignment::Unaligned => (),
        };
    }

    #[test]
    fn test_enums_serialize_round_trip() {
        for ct in [CreatureType::Beast, CreatureType::Undead, CreatureType::Fey] {
            let json = serde_json::to_string(&ct).unwrap();
            let back: CreatureType = serde_json::from_str(&json).unwrap();
            assert_eq!(back, ct);
        }
        for sz in [Size::Tiny, Size::Medium, Size::Gargantuan] {
            let json = serde_json::to_string(&sz).unwrap();
            let back: Size = serde_json::from_str(&json).unwrap();
            assert_eq!(back, sz);
        }
        for al in [Alignment::ChaoticEvil, Alignment::LawfulGood, Alignment::Unaligned] {
            let json = serde_json::to_string(&al).unwrap();
            let back: Alignment = serde_json::from_str(&json).unwrap();
            assert_eq!(back, al);
        }
    }

    #[test]
    fn test_skeleton_full_stat_block() {
        let s = find_monster("Skeleton").unwrap();
        assert_eq!(s.creature_type, CreatureType::Undead);
        assert_eq!(s.size, Size::Medium);
        assert_eq!(s.alignment, Alignment::LawfulEvil);
        assert!(s.damage_immunities.contains(&DamageType::Poison),
            "Skeleton should be immune to Poison damage");
        assert!(s.condition_immunities.contains(&ConditionType::Poisoned),
            "Skeleton should be immune to Poisoned condition");
        assert!(s.condition_immunities.contains(&ConditionType::Exhaustion),
            "Skeleton should be immune to Exhaustion");
        assert!(s.languages.iter().any(|l| l.contains("Common")),
            "Skeleton should understand Common");
        assert_eq!(s.multiattack, 1);
    }

    #[test]
    fn test_zombie_full_stat_block() {
        let z = find_monster("Zombie").unwrap();
        assert_eq!(z.creature_type, CreatureType::Undead);
        assert_eq!(z.alignment, Alignment::NeutralEvil);
        assert!(z.damage_immunities.contains(&DamageType::Poison));
        assert!(z.condition_immunities.contains(&ConditionType::Poisoned));
        assert!(z.condition_immunities.contains(&ConditionType::Exhaustion));
    }

    #[test]
    fn test_ghoul_multiattack_and_immunities() {
        let g = find_monster("Ghoul").unwrap();
        assert_eq!(g.creature_type, CreatureType::Undead);
        assert_eq!(g.alignment, Alignment::ChaoticEvil);
        assert!(g.damage_immunities.contains(&DamageType::Poison));
        assert!(g.condition_immunities.contains(&ConditionType::Charmed));
        assert!(g.condition_immunities.contains(&ConditionType::Exhaustion));
        assert!(g.condition_immunities.contains(&ConditionType::Poisoned));
        assert_eq!(g.multiattack, 2,
            "Ghoul should have multiattack 2 (two Bite attacks per Attack action)");
    }

    #[test]
    fn test_bugbear_multiattack() {
        let b = find_monster("Bugbear").unwrap();
        assert_eq!(b.creature_type, CreatureType::Fey);
        assert_eq!(b.size, Size::Medium);
        assert_eq!(b.alignment, Alignment::ChaoticEvil);
        assert_eq!(b.multiattack, 2);
    }

    #[test]
    fn test_ogre_size_giant() {
        let o = find_monster("Ogre").unwrap();
        assert_eq!(o.creature_type, CreatureType::Giant);
        assert_eq!(o.size, Size::Large);
        assert_eq!(o.alignment, Alignment::ChaoticEvil);
        assert_eq!(o.multiattack, 1);
    }

    #[test]
    fn test_kobold_dragon_type() {
        let k = find_monster("Kobold").unwrap();
        assert_eq!(k.creature_type, CreatureType::Dragon,
            "2024 SRD lists Kobold Warrior as Small Dragon");
        assert_eq!(k.size, Size::Small);
    }

    #[test]
    fn test_goblin_fey_type() {
        let g = find_monster("Goblin").unwrap();
        assert_eq!(g.creature_type, CreatureType::Fey,
            "2024 SRD lists Goblin Warrior as Small Fey (Goblinoid)");
        assert_eq!(g.size, Size::Small);
    }

    #[test]
    fn test_rat_beast() {
        let r = find_monster("Rat").unwrap();
        assert_eq!(r.creature_type, CreatureType::Beast);
        assert_eq!(r.size, Size::Tiny);
        assert_eq!(r.alignment, Alignment::Unaligned);
    }

    #[test]
    fn test_all_monsters_have_at_least_multiattack_1() {
        for m in SRD_MONSTERS {
            assert!(m.multiattack >= 1, "{} has multiattack < 1", m.name);
        }
    }

    #[test]
    fn test_canonical_monster_crs() {
        // Spot-check: all entries from the leveling spec table.
        assert_eq!(find_monster("Rat").unwrap().cr, 0.0);
        assert_eq!(find_monster("Kobold").unwrap().cr, 0.125);
        assert_eq!(find_monster("Goblin").unwrap().cr, 0.25);
        assert_eq!(find_monster("Skeleton").unwrap().cr, 0.25);
        assert_eq!(find_monster("Zombie").unwrap().cr, 0.25);
        assert_eq!(find_monster("Guard").unwrap().cr, 0.125);
        assert_eq!(find_monster("Bandit").unwrap().cr, 0.125);
        assert_eq!(find_monster("Orc").unwrap().cr, 0.5);
        assert_eq!(find_monster("Hobgoblin").unwrap().cr, 0.5);
        assert_eq!(find_monster("Bugbear").unwrap().cr, 1.0);
        assert_eq!(find_monster("Ghoul").unwrap().cr, 1.0);
        assert_eq!(find_monster("Ogre").unwrap().cr, 2.0);
    }

}
