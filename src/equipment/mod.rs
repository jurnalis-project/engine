// jurnalis-engine/src/equipment/mod.rs
pub mod magic;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::ItemId;
use crate::state::{DamageType, WeaponCategory, ArmorCategory, Item, ItemType};
use crate::character::Character;
use crate::types::Ability;

// -- Weapon property bitflags --
pub const FINESSE: u16    = 1 << 0;
pub const LIGHT: u16      = 1 << 1;
pub const TWO_HANDED: u16 = 1 << 2;
pub const VERSATILE: u16  = 1 << 3;
pub const THROWN: u16     = 1 << 4;
pub const HEAVY: u16      = 1 << 5;
pub const REACH: u16      = 1 << 6;
pub const LOADING: u16    = 1 << 7;
pub const AMMUNITION: u16 = 1 << 8;
pub const SPECIAL: u16    = 1 << 9;

// -- SRD Weapon Definition --
pub struct WeaponDef {
    pub name: &'static str,
    pub category: WeaponCategory,
    pub cost_cp: u32,
    pub damage_dice: u32,
    pub damage_die: u32,
    pub damage_type: DamageType,
    pub weight_qp: u32,  // quarter-pounds
    pub properties: u16,
    pub versatile_die: u32,
    pub range_normal: u32,
    pub range_long: u32,
}

// -- SRD Armor Definition --
pub struct ArmorDef {
    pub name: &'static str,
    pub category: ArmorCategory,
    pub base_ac: u32,
    pub max_dex_bonus: Option<u32>,
    pub str_requirement: u32,
    pub stealth_disadvantage: bool,
    pub cost_cp: u32,
    pub weight_qp: u32,
}

// -- Equipment Slots --
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Equipment {
    pub main_hand: Option<ItemId>,
    pub off_hand: Option<ItemId>,
    pub body: Option<ItemId>,
}

pub const SRD_WEAPONS: &[WeaponDef] = &[
    // === Simple Melee ===
    WeaponDef { name: "Club", category: WeaponCategory::Simple, cost_cp: 10, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: LIGHT, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Dagger", category: WeaponCategory::Simple, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Piercing, weight_qp: 4, properties: FINESSE | LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60 },
    WeaponDef { name: "Greatclub", category: WeaponCategory::Simple, cost_cp: 20, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 40, properties: TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Handaxe", category: WeaponCategory::Simple, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 8, properties: LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60 },
    WeaponDef { name: "Javelin", category: WeaponCategory::Simple, cost_cp: 50, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: THROWN, versatile_die: 0, range_normal: 30, range_long: 120 },
    WeaponDef { name: "Light Hammer", category: WeaponCategory::Simple, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60 },
    WeaponDef { name: "Mace", category: WeaponCategory::Simple, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 16, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Quarterstaff", category: WeaponCategory::Simple, cost_cp: 20, damage_dice: 1, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 16, properties: VERSATILE, versatile_die: 8, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Sickle", category: WeaponCategory::Simple, cost_cp: 100, damage_dice: 1, damage_die: 4, damage_type: DamageType::Slashing, weight_qp: 8, properties: LIGHT, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Spear", category: WeaponCategory::Simple, cost_cp: 100, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 12, properties: THROWN | VERSATILE, versatile_die: 8, range_normal: 20, range_long: 60 },
    // === Simple Ranged ===
    WeaponDef { name: "Light Crossbow", category: WeaponCategory::Simple, cost_cp: 2500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 20, properties: AMMUNITION | LOADING | TWO_HANDED, versatile_die: 0, range_normal: 80, range_long: 320 },
    WeaponDef { name: "Dart", category: WeaponCategory::Simple, cost_cp: 5, damage_dice: 1, damage_die: 4, damage_type: DamageType::Piercing, weight_qp: 1, properties: FINESSE | THROWN, versatile_die: 0, range_normal: 20, range_long: 60 },
    WeaponDef { name: "Shortbow", category: WeaponCategory::Simple, cost_cp: 2500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: AMMUNITION | TWO_HANDED, versatile_die: 0, range_normal: 80, range_long: 320 },
    WeaponDef { name: "Sling", category: WeaponCategory::Simple, cost_cp: 10, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 0, properties: AMMUNITION, versatile_die: 0, range_normal: 30, range_long: 120 },
    // === Martial Melee ===
    WeaponDef { name: "Battleaxe", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Slashing, weight_qp: 16, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Flail", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Glaive", category: WeaponCategory::Martial, cost_cp: 2000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Greataxe", category: WeaponCategory::Martial, cost_cp: 3000, damage_dice: 1, damage_die: 12, damage_type: DamageType::Slashing, weight_qp: 28, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Greatsword", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 2, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Halberd", category: WeaponCategory::Martial, cost_cp: 2000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Lance", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 12, damage_type: DamageType::Piercing, weight_qp: 24, properties: REACH | SPECIAL, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Longsword", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Slashing, weight_qp: 12, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Maul", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 2, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 40, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Morningstar", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 16, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Pike", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 10, damage_type: DamageType::Piercing, weight_qp: 72, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Rapier", category: WeaponCategory::Martial, cost_cp: 2500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: FINESSE, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Scimitar", category: WeaponCategory::Martial, cost_cp: 2500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 12, properties: FINESSE | LIGHT, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Shortsword", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: FINESSE | LIGHT, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Trident", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 16, properties: THROWN | VERSATILE, versatile_die: 8, range_normal: 20, range_long: 60 },
    WeaponDef { name: "War Pick", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Warhammer", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0 },
    WeaponDef { name: "Whip", category: WeaponCategory::Martial, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Slashing, weight_qp: 12, properties: FINESSE | REACH, versatile_die: 0, range_normal: 0, range_long: 0 },
    // === Martial Ranged ===
    WeaponDef { name: "Blowgun", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 1, damage_type: DamageType::Piercing, weight_qp: 4, properties: AMMUNITION | LOADING, versatile_die: 0, range_normal: 25, range_long: 100 },
    WeaponDef { name: "Hand Crossbow", category: WeaponCategory::Martial, cost_cp: 7500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 12, properties: AMMUNITION | LIGHT | LOADING, versatile_die: 0, range_normal: 30, range_long: 120 },
    WeaponDef { name: "Heavy Crossbow", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Piercing, weight_qp: 72, properties: AMMUNITION | HEAVY | LOADING | TWO_HANDED, versatile_die: 0, range_normal: 100, range_long: 400 },
    WeaponDef { name: "Longbow", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: AMMUNITION | HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 150, range_long: 600 },
    WeaponDef { name: "Net", category: WeaponCategory::Martial, cost_cp: 100, damage_dice: 0, damage_die: 0, damage_type: DamageType::Bludgeoning, weight_qp: 12, properties: SPECIAL | THROWN, versatile_die: 0, range_normal: 5, range_long: 15 },
];

pub const SRD_ARMOR: &[ArmorDef] = &[
    // === Light Armor ===
    ArmorDef { name: "Padded", category: ArmorCategory::Light, base_ac: 11, max_dex_bonus: None, str_requirement: 0, stealth_disadvantage: true, cost_cp: 500, weight_qp: 32 },
    ArmorDef { name: "Leather", category: ArmorCategory::Light, base_ac: 11, max_dex_bonus: None, str_requirement: 0, stealth_disadvantage: false, cost_cp: 1000, weight_qp: 40 },
    ArmorDef { name: "Studded Leather", category: ArmorCategory::Light, base_ac: 12, max_dex_bonus: None, str_requirement: 0, stealth_disadvantage: false, cost_cp: 4500, weight_qp: 52 },
    // === Medium Armor ===
    ArmorDef { name: "Hide", category: ArmorCategory::Medium, base_ac: 12, max_dex_bonus: Some(2), str_requirement: 0, stealth_disadvantage: false, cost_cp: 1000, weight_qp: 48 },
    ArmorDef { name: "Chain Shirt", category: ArmorCategory::Medium, base_ac: 13, max_dex_bonus: Some(2), str_requirement: 0, stealth_disadvantage: false, cost_cp: 5000, weight_qp: 80 },
    ArmorDef { name: "Scale Mail", category: ArmorCategory::Medium, base_ac: 14, max_dex_bonus: Some(2), str_requirement: 0, stealth_disadvantage: true, cost_cp: 5000, weight_qp: 180 },
    ArmorDef { name: "Breastplate", category: ArmorCategory::Medium, base_ac: 14, max_dex_bonus: Some(2), str_requirement: 0, stealth_disadvantage: false, cost_cp: 40000, weight_qp: 80 },
    ArmorDef { name: "Half Plate", category: ArmorCategory::Medium, base_ac: 15, max_dex_bonus: Some(2), str_requirement: 0, stealth_disadvantage: true, cost_cp: 75000, weight_qp: 160 },
    // === Heavy Armor ===
    ArmorDef { name: "Ring Mail", category: ArmorCategory::Heavy, base_ac: 14, max_dex_bonus: Some(0), str_requirement: 0, stealth_disadvantage: true, cost_cp: 3000, weight_qp: 160 },
    ArmorDef { name: "Chain Mail", category: ArmorCategory::Heavy, base_ac: 16, max_dex_bonus: Some(0), str_requirement: 13, stealth_disadvantage: true, cost_cp: 7500, weight_qp: 220 },
    ArmorDef { name: "Splint", category: ArmorCategory::Heavy, base_ac: 17, max_dex_bonus: Some(0), str_requirement: 15, stealth_disadvantage: true, cost_cp: 20000, weight_qp: 240 },
    ArmorDef { name: "Plate", category: ArmorCategory::Heavy, base_ac: 18, max_dex_bonus: Some(0), str_requirement: 15, stealth_disadvantage: true, cost_cp: 150000, weight_qp: 260 },
    // === Shield ===
    ArmorDef { name: "Shield", category: ArmorCategory::Shield, base_ac: 2, max_dex_bonus: None, str_requirement: 0, stealth_disadvantage: false, cost_cp: 1000, weight_qp: 24 },
];

pub fn calculate_ac(character: &Character, items: &HashMap<ItemId, Item>) -> i32 {
    let dex_mod = character.ability_modifier(Ability::Dexterity);

    let base_ac = match character.equipped.body {
        Some(body_id) => {
            match items.get(&body_id).map(|i| &i.item_type) {
                Some(ItemType::Armor { base_ac, max_dex_bonus, .. }) => {
                    let dex_contribution = match max_dex_bonus {
                        None => dex_mod,
                        Some(0) => 0,
                        Some(cap) => dex_mod.min(*cap as i32),
                    };
                    *base_ac as i32 + dex_contribution
                }
                _ => 10 + dex_mod,
            }
        }
        None => 10 + dex_mod,
    };

    let shield_bonus = match character.equipped.off_hand {
        Some(oh_id) => {
            match items.get(&oh_id).map(|i| &i.item_type) {
                Some(ItemType::Armor { category: ArmorCategory::Shield, base_ac, .. }) => *base_ac as i32,
                _ => 0,
            }
        }
        None => 0,
    };

    base_ac + shield_bonus
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::types::Ability;
    use crate::character::{create_character, race::Race, class::Class};
    use crate::state::{Item, ItemType, ArmorCategory};

    fn test_character(dex: i32) -> crate::character::Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 10);
        scores.insert(Ability::Dexterity, dex);
        scores.insert(Ability::Constitution, 10);
        scores.insert(Ability::Intelligence, 10);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 10);
        create_character("Test".to_string(), Race::Human, Class::Fighter, scores, vec![])
    }

    fn make_armor(id: u32, name: &str, category: ArmorCategory, base_ac: u32, max_dex: Option<u32>) -> Item {
        Item {
            id, name: name.to_string(), description: String::new(),
            item_type: ItemType::Armor {
                category, base_ac, max_dex_bonus: max_dex,
                str_requirement: 0, stealth_disadvantage: false,
            },
            location: None, carried_by_player: true,
        }
    }

    #[test]
    fn test_ac_no_armor() {
        let c = test_character(14); // DEX 14+1(human) = 15, mod = +2
        let items = HashMap::new();
        assert_eq!(calculate_ac(&c, &items), 12); // 10 + 2
    }

    #[test]
    fn test_ac_light_armor() {
        let mut c = test_character(16); // DEX 16+1 = 17, mod = +3
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Leather", ArmorCategory::Light, 11, None));
        c.equipped.body = Some(0);
        assert_eq!(calculate_ac(&c, &items), 14); // 11 + 3
    }

    #[test]
    fn test_ac_medium_armor_caps_dex() {
        let mut c = test_character(16); // mod = +3
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Breastplate", ArmorCategory::Medium, 14, Some(2)));
        c.equipped.body = Some(0);
        assert_eq!(calculate_ac(&c, &items), 16); // 14 + min(3, 2) = 16
    }

    #[test]
    fn test_ac_heavy_armor_ignores_dex() {
        let mut c = test_character(16); // mod = +3
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Chain Mail", ArmorCategory::Heavy, 16, Some(0)));
        c.equipped.body = Some(0);
        assert_eq!(calculate_ac(&c, &items), 16); // 16 flat
    }

    #[test]
    fn test_ac_with_shield() {
        let mut c = test_character(14); // mod = +2
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Shield", ArmorCategory::Shield, 2, None));
        c.equipped.off_hand = Some(0);
        assert_eq!(calculate_ac(&c, &items), 14); // 10 + 2 (dex) + 2 (shield)
    }

    #[test]
    fn test_ac_armor_plus_shield() {
        let mut c = test_character(14); // mod = +2
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Chain Mail", ArmorCategory::Heavy, 16, Some(0)));
        items.insert(1, make_armor(1, "Shield", ArmorCategory::Shield, 2, None));
        c.equipped.body = Some(0);
        c.equipped.off_hand = Some(1);
        assert_eq!(calculate_ac(&c, &items), 18); // 16 + 2
    }

    #[test]
    fn test_ac_heavy_armor_ignores_negative_dex() {
        let mut c = test_character(6); // DEX 6+1(human) = 7, mod = -2
        let mut items = HashMap::new();
        items.insert(0, make_armor(0, "Chain Mail", ArmorCategory::Heavy, 16, Some(0)));
        c.equipped.body = Some(0);
        assert_eq!(calculate_ac(&c, &items), 16); // -2 DEX must not reduce AC
    }

    #[test]
    fn test_srd_weapons_count() {
        assert_eq!(SRD_WEAPONS.len(), 37);
    }

    #[test]
    fn test_srd_armor_count() {
        assert_eq!(SRD_ARMOR.len(), 13);
    }

    #[test]
    fn test_dagger_properties() {
        let dagger = SRD_WEAPONS.iter().find(|w| w.name == "Dagger").unwrap();
        assert_eq!(dagger.category, WeaponCategory::Simple);
        assert_eq!(dagger.damage_die, 4);
        assert_eq!(dagger.damage_type, DamageType::Piercing);
        assert!(dagger.properties & FINESSE != 0);
        assert!(dagger.properties & LIGHT != 0);
        assert!(dagger.properties & THROWN != 0);
        assert_eq!(dagger.range_normal, 20);
        assert_eq!(dagger.range_long, 60);
    }

    #[test]
    fn test_greatsword_properties() {
        let gs = SRD_WEAPONS.iter().find(|w| w.name == "Greatsword").unwrap();
        assert_eq!(gs.category, WeaponCategory::Martial);
        assert_eq!(gs.damage_dice, 2);
        assert_eq!(gs.damage_die, 6);
        assert_eq!(gs.damage_type, DamageType::Slashing);
        assert!(gs.properties & HEAVY != 0);
        assert!(gs.properties & TWO_HANDED != 0);
    }

    #[test]
    fn test_longsword_versatile() {
        let ls = SRD_WEAPONS.iter().find(|w| w.name == "Longsword").unwrap();
        assert!(ls.properties & VERSATILE != 0);
        assert_eq!(ls.damage_die, 8);
        assert_eq!(ls.versatile_die, 10);
    }

    #[test]
    fn test_chain_mail_properties() {
        let cm = SRD_ARMOR.iter().find(|a| a.name == "Chain Mail").unwrap();
        assert_eq!(cm.category, ArmorCategory::Heavy);
        assert_eq!(cm.base_ac, 16);
        assert_eq!(cm.max_dex_bonus, Some(0));
        assert_eq!(cm.str_requirement, 13);
        assert!(cm.stealth_disadvantage);
    }

    #[test]
    fn test_leather_armor_properties() {
        let leather = SRD_ARMOR.iter().find(|a| a.name == "Leather").unwrap();
        assert_eq!(leather.category, ArmorCategory::Light);
        assert_eq!(leather.base_ac, 11);
        assert_eq!(leather.max_dex_bonus, None); // unlimited DEX
        assert!(!leather.stealth_disadvantage);
    }

    #[test]
    fn test_shield_properties() {
        let shield = SRD_ARMOR.iter().find(|a| a.name == "Shield").unwrap();
        assert_eq!(shield.category, ArmorCategory::Shield);
        assert_eq!(shield.base_ac, 2);
    }

    #[test]
    fn test_net_zero_damage() {
        let net = SRD_WEAPONS.iter().find(|w| w.name == "Net").unwrap();
        assert_eq!(net.damage_dice, 0);
        assert_eq!(net.damage_die, 0);
        assert!(net.properties & SPECIAL != 0);
        assert!(net.properties & THROWN != 0);
    }

    #[test]
    fn test_equipment_default_empty() {
        let eq = Equipment::default();
        assert!(eq.main_hand.is_none());
        assert!(eq.off_hand.is_none());
        assert!(eq.body.is_none());
    }
}
