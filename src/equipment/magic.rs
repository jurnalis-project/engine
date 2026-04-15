// jurnalis-engine/src/equipment/magic.rs
//
// Magic item foundations: rarity tiers, effect kinds, and SRD magic-item
// const tables. This module is pure-data — it does NOT import from
// character/, combat/, parser/, narration/, or spells/. All cross-module
// orchestration (applying bonuses, resolving use) is handled by lib.rs.

use serde::{Deserialize, Serialize};

/// SRD 5.1 magic item rarity tiers. Ordered from most common to most rare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    VeryRare,
    Legendary,
    Artifact,
}

/// Maximum number of items a character may be attuned to simultaneously.
/// Per SRD 5.1.
pub const MAX_ATTUNED_ITEMS: usize = 3;

/// Wondrous item effects. Kept coarse-grained for MVP; many variants are
/// currently flavor-only (documented in docs/specs/magic-items.md under
/// "Deferred / Out of Scope").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WondrousEffect {
    /// Flavor-only for MVP (no encumbrance system).
    BagOfHolding,
    /// While attuned: +1 AC and +1 to all saving throws.
    CloakOfProtection,
    /// While attuned: +1 AC and +1 to all saving throws.
    RingOfProtection,
    /// Flavor-only for MVP (no active-buff timer system).
    BootsOfSpeed,
    /// While attuned: sets effective STR to 19 (if wearer's natural STR is
    /// lower). No effect otherwise.
    GauntletsOfOgrePower,
    /// While attuned: sets effective STR to the embedded score (if wearer's
    /// natural STR is lower). MVP uses the Hill Giant variant (21).
    BeltOfGiantStrength(u32),
}

/// Potion effect variants. Healing is mechanically implemented; others are
/// flavor-only for MVP.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PotionEffect {
    /// Roll `dice`d`die` + `bonus` HP and heal (capped at max_hp).
    Healing { dice: u32, die: u32, bonus: i32 },
    /// Flavor-only for MVP.
    Speed,
    /// Flavor-only for MVP.
    Invisibility,
    /// Flavor-only for MVP.
    Climbing,
}

/// A magic item's mechanical category, paired with its type-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MagicItemKind {
    /// A magical weapon atop a base weapon from `SRD_WEAPONS`.
    MagicWeapon {
        base_weapon: &'static str,
        attack_bonus: i32,
        damage_bonus: i32,
    },
    /// A magical armor atop a base armor from `SRD_ARMOR`.
    MagicArmor {
        base_armor: &'static str,
        ac_bonus: i32,
    },
    /// A wondrous item with a fixed effect.
    Wondrous { effect: WondrousEffect },
    /// A one-shot potion.
    Potion { effect: PotionEffect },
    /// A one-shot spell scroll.
    Scroll {
        spell_name: &'static str,
        spell_level: u32,
    },
    /// A charged wand.
    Wand {
        spell_name: &'static str,
        charges_max: u32,
    },
}

/// Compile-time definition of a magic item: name, rarity, attunement
/// requirement, and kind-specific data.
#[derive(Debug, Clone, PartialEq)]
pub struct MagicItemDef {
    pub name: &'static str,
    pub rarity: Rarity,
    pub requires_attunement: bool,
    pub kind: MagicItemKind,
}

/// SRD 5.1 core magic weapons. Only mechanically-modelled weapons are
/// included; deferred variants (Flame Tongue, Vorpal Sword, Holy Avenger)
/// are documented in docs/specs/magic-items.md.
pub const SRD_MAGIC_WEAPONS: &[MagicItemDef] = &[
    // +1 / +2 / +3 Longsword
    MagicItemDef { name: "+1 Longsword", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Longsword", attack_bonus: 1, damage_bonus: 1 } },
    MagicItemDef { name: "+2 Longsword", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Longsword", attack_bonus: 2, damage_bonus: 2 } },
    MagicItemDef { name: "+3 Longsword", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Longsword", attack_bonus: 3, damage_bonus: 3 } },
    // +1 / +2 / +3 Shortsword
    MagicItemDef { name: "+1 Shortsword", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Shortsword", attack_bonus: 1, damage_bonus: 1 } },
    MagicItemDef { name: "+2 Shortsword", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Shortsword", attack_bonus: 2, damage_bonus: 2 } },
    MagicItemDef { name: "+3 Shortsword", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Shortsword", attack_bonus: 3, damage_bonus: 3 } },
    // +1 / +2 / +3 Dagger
    MagicItemDef { name: "+1 Dagger", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Dagger", attack_bonus: 1, damage_bonus: 1 } },
    MagicItemDef { name: "+2 Dagger", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Dagger", attack_bonus: 2, damage_bonus: 2 } },
    MagicItemDef { name: "+3 Dagger", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Dagger", attack_bonus: 3, damage_bonus: 3 } },
    // +1 / +2 / +3 Handaxe
    MagicItemDef { name: "+1 Handaxe", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Handaxe", attack_bonus: 1, damage_bonus: 1 } },
    MagicItemDef { name: "+2 Handaxe", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Handaxe", attack_bonus: 2, damage_bonus: 2 } },
    MagicItemDef { name: "+3 Handaxe", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicWeapon { base_weapon: "Handaxe", attack_bonus: 3, damage_bonus: 3 } },
];

/// SRD 5.1 core magic armor.
pub const SRD_MAGIC_ARMOR: &[MagicItemDef] = &[
    // +1 / +2 / +3 Chain Mail
    MagicItemDef { name: "+1 Chain Mail", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Chain Mail", ac_bonus: 1 } },
    MagicItemDef { name: "+2 Chain Mail", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Chain Mail", ac_bonus: 2 } },
    MagicItemDef { name: "+3 Chain Mail", rarity: Rarity::Legendary, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Chain Mail", ac_bonus: 3 } },
    // +1 / +2 / +3 Plate
    MagicItemDef { name: "+1 Plate", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Plate", ac_bonus: 1 } },
    MagicItemDef { name: "+2 Plate", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Plate", ac_bonus: 2 } },
    MagicItemDef { name: "+3 Plate", rarity: Rarity::Legendary, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Plate", ac_bonus: 3 } },
    // +1 / +2 / +3 Leather
    MagicItemDef { name: "+1 Leather", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Leather", ac_bonus: 1 } },
    MagicItemDef { name: "+2 Leather", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Leather", ac_bonus: 2 } },
    MagicItemDef { name: "+3 Leather", rarity: Rarity::Legendary, requires_attunement: false,
        kind: MagicItemKind::MagicArmor { base_armor: "Leather", ac_bonus: 3 } },
];

/// SRD 5.1 core wondrous items.
pub const SRD_WONDROUS: &[MagicItemDef] = &[
    MagicItemDef { name: "Bag of Holding", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::BagOfHolding } },
    MagicItemDef { name: "Cloak of Protection", rarity: Rarity::Uncommon, requires_attunement: true,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::CloakOfProtection } },
    MagicItemDef { name: "Ring of Protection", rarity: Rarity::Rare, requires_attunement: true,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::RingOfProtection } },
    MagicItemDef { name: "Boots of Speed", rarity: Rarity::Rare, requires_attunement: true,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::BootsOfSpeed } },
    MagicItemDef { name: "Gauntlets of Ogre Power", rarity: Rarity::Uncommon, requires_attunement: true,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::GauntletsOfOgrePower } },
    MagicItemDef { name: "Belt of Giant Strength (Hill)", rarity: Rarity::Rare, requires_attunement: true,
        kind: MagicItemKind::Wondrous { effect: WondrousEffect::BeltOfGiantStrength(21) } },
];

/// SRD 5.1 core potions.
pub const SRD_POTIONS: &[MagicItemDef] = &[
    MagicItemDef { name: "Potion of Healing", rarity: Rarity::Common, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Healing { dice: 2, die: 4, bonus: 2 } } },
    MagicItemDef { name: "Potion of Greater Healing", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Healing { dice: 4, die: 4, bonus: 4 } } },
    MagicItemDef { name: "Potion of Superior Healing", rarity: Rarity::Rare, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Healing { dice: 8, die: 4, bonus: 8 } } },
    MagicItemDef { name: "Potion of Supreme Healing", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Healing { dice: 10, die: 4, bonus: 20 } } },
    MagicItemDef { name: "Potion of Speed", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Speed } },
    MagicItemDef { name: "Potion of Invisibility", rarity: Rarity::VeryRare, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Invisibility } },
    MagicItemDef { name: "Potion of Climbing", rarity: Rarity::Common, requires_attunement: false,
        kind: MagicItemKind::Potion { effect: PotionEffect::Climbing } },
];

/// SRD 5.1 core scrolls. `spell_name` is free-form; actual spell resolution
/// is deferred (MVP narrates only).
pub const SRD_SCROLLS: &[MagicItemDef] = &[
    MagicItemDef { name: "Scroll of Magic Missile", rarity: Rarity::Common, requires_attunement: false,
        kind: MagicItemKind::Scroll { spell_name: "Magic Missile", spell_level: 1 } },
    MagicItemDef { name: "Scroll of Fireball", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::Scroll { spell_name: "Fireball", spell_level: 3 } },
    MagicItemDef { name: "Scroll of Cure Wounds", rarity: Rarity::Common, requires_attunement: false,
        kind: MagicItemKind::Scroll { spell_name: "Cure Wounds", spell_level: 1 } },
];

/// SRD 5.1 core wands. All carry 7 charges in this MVP.
pub const SRD_WANDS: &[MagicItemDef] = &[
    MagicItemDef { name: "Wand of Magic Missiles", rarity: Rarity::Uncommon, requires_attunement: false,
        kind: MagicItemKind::Wand { spell_name: "Magic Missile", charges_max: 7 } },
    MagicItemDef { name: "Wand of Fireballs", rarity: Rarity::Rare, requires_attunement: true,
        kind: MagicItemKind::Wand { spell_name: "Fireball", charges_max: 7 } },
    MagicItemDef { name: "Wand of Lightning Bolts", rarity: Rarity::Rare, requires_attunement: true,
        kind: MagicItemKind::Wand { spell_name: "Lightning Bolt", charges_max: 7 } },
];

/// Look up a magic-item definition by name across all SRD tables.
pub fn find_magic_item(name: &str) -> Option<&'static MagicItemDef> {
    SRD_MAGIC_WEAPONS.iter()
        .chain(SRD_MAGIC_ARMOR.iter())
        .chain(SRD_WONDROUS.iter())
        .chain(SRD_POTIONS.iter())
        .chain(SRD_SCROLLS.iter())
        .chain(SRD_WANDS.iter())
        .find(|d| d.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magic_weapons_count() {
        // 4 base weapons * 3 bonuses each = 12 magic weapon definitions.
        assert_eq!(SRD_MAGIC_WEAPONS.len(), 12);
    }

    #[test]
    fn test_magic_armor_count() {
        // 3 base armors * 3 bonuses each = 9 magic armor definitions.
        assert_eq!(SRD_MAGIC_ARMOR.len(), 9);
    }

    #[test]
    fn test_wondrous_count() {
        assert_eq!(SRD_WONDROUS.len(), 6);
    }

    #[test]
    fn test_potions_count() {
        assert_eq!(SRD_POTIONS.len(), 7);
    }

    #[test]
    fn test_scrolls_count() {
        assert_eq!(SRD_SCROLLS.len(), 3);
    }

    #[test]
    fn test_wands_count() {
        assert_eq!(SRD_WANDS.len(), 3);
    }

    #[test]
    fn test_max_attuned_items_is_three() {
        assert_eq!(MAX_ATTUNED_ITEMS, 3);
    }

    #[test]
    fn test_plus_one_longsword_is_uncommon_no_attune() {
        let def = find_magic_item("+1 Longsword").unwrap();
        assert_eq!(def.rarity, Rarity::Uncommon);
        assert!(!def.requires_attunement);
        match def.kind {
            MagicItemKind::MagicWeapon { base_weapon, attack_bonus, damage_bonus } => {
                assert_eq!(base_weapon, "Longsword");
                assert_eq!(attack_bonus, 1);
                assert_eq!(damage_bonus, 1);
            }
            _ => panic!("expected MagicWeapon"),
        }
    }

    #[test]
    fn test_cloak_of_protection_requires_attunement() {
        let def = find_magic_item("Cloak of Protection").unwrap();
        assert!(def.requires_attunement);
        assert_eq!(def.rarity, Rarity::Uncommon);
    }

    #[test]
    fn test_healing_potion_tiers() {
        let def = find_magic_item("Potion of Healing").unwrap();
        match def.kind {
            MagicItemKind::Potion { effect: PotionEffect::Healing { dice, die, bonus } } => {
                assert_eq!(dice, 2); assert_eq!(die, 4); assert_eq!(bonus, 2);
            }
            _ => panic!("expected Healing potion"),
        }
        let def = find_magic_item("Potion of Greater Healing").unwrap();
        match def.kind {
            MagicItemKind::Potion { effect: PotionEffect::Healing { dice, die, bonus } } => {
                assert_eq!(dice, 4); assert_eq!(die, 4); assert_eq!(bonus, 4);
            }
            _ => panic!("expected Healing potion"),
        }
        let def = find_magic_item("Potion of Superior Healing").unwrap();
        match def.kind {
            MagicItemKind::Potion { effect: PotionEffect::Healing { dice, die, bonus } } => {
                assert_eq!(dice, 8); assert_eq!(die, 4); assert_eq!(bonus, 8);
            }
            _ => panic!("expected Healing potion"),
        }
        let def = find_magic_item("Potion of Supreme Healing").unwrap();
        match def.kind {
            MagicItemKind::Potion { effect: PotionEffect::Healing { dice, die, bonus } } => {
                assert_eq!(dice, 10); assert_eq!(die, 4); assert_eq!(bonus, 20);
            }
            _ => panic!("expected Healing potion"),
        }
    }

    #[test]
    fn test_wand_of_magic_missiles_charges() {
        let def = find_magic_item("Wand of Magic Missiles").unwrap();
        match def.kind {
            MagicItemKind::Wand { spell_name, charges_max } => {
                assert_eq!(spell_name, "Magic Missile");
                assert_eq!(charges_max, 7);
            }
            _ => panic!("expected Wand"),
        }
    }

    #[test]
    fn test_find_unknown_returns_none() {
        assert!(find_magic_item("Sword of Nonsense").is_none());
    }

    #[test]
    fn test_rarity_equality_and_copy() {
        let r: Rarity = Rarity::Rare;
        let r2 = r; // Copy
        assert_eq!(r, r2);
    }
}
