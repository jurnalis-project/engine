// jurnalis-engine/src/equipment/mod.rs
pub mod magic;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::types::{ItemId, Mastery};
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
    /// Weapon Mastery property per 2024 SRD (docs/reference/equipment.md).
    /// Every weapon has exactly one mastery. Characters only benefit from
    /// it if they have unlocked mastery for that weapon (see
    /// `docs/specs/weapon-mastery.md`).
    pub mastery: Mastery,
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
    WeaponDef { name: "Club", category: WeaponCategory::Simple, cost_cp: 10, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: LIGHT, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Slow },
    WeaponDef { name: "Dagger", category: WeaponCategory::Simple, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Piercing, weight_qp: 4, properties: FINESSE | LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60, mastery: Mastery::Nick },
    WeaponDef { name: "Greatclub", category: WeaponCategory::Simple, cost_cp: 20, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 40, properties: TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Push },
    WeaponDef { name: "Handaxe", category: WeaponCategory::Simple, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 8, properties: LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60, mastery: Mastery::Vex },
    WeaponDef { name: "Javelin", category: WeaponCategory::Simple, cost_cp: 50, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: THROWN, versatile_die: 0, range_normal: 30, range_long: 120, mastery: Mastery::Slow },
    WeaponDef { name: "Light Hammer", category: WeaponCategory::Simple, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: LIGHT | THROWN, versatile_die: 0, range_normal: 20, range_long: 60, mastery: Mastery::Nick },
    WeaponDef { name: "Mace", category: WeaponCategory::Simple, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 16, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Sap },
    WeaponDef { name: "Quarterstaff", category: WeaponCategory::Simple, cost_cp: 20, damage_dice: 1, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 16, properties: VERSATILE, versatile_die: 8, range_normal: 0, range_long: 0, mastery: Mastery::Topple },
    WeaponDef { name: "Sickle", category: WeaponCategory::Simple, cost_cp: 100, damage_dice: 1, damage_die: 4, damage_type: DamageType::Slashing, weight_qp: 8, properties: LIGHT, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Nick },
    WeaponDef { name: "Spear", category: WeaponCategory::Simple, cost_cp: 100, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 12, properties: THROWN | VERSATILE, versatile_die: 8, range_normal: 20, range_long: 60, mastery: Mastery::Sap },
    // === Simple Ranged ===
    WeaponDef { name: "Light Crossbow", category: WeaponCategory::Simple, cost_cp: 2500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 20, properties: AMMUNITION | LOADING | TWO_HANDED, versatile_die: 0, range_normal: 80, range_long: 320, mastery: Mastery::Slow },
    WeaponDef { name: "Dart", category: WeaponCategory::Simple, cost_cp: 5, damage_dice: 1, damage_die: 4, damage_type: DamageType::Piercing, weight_qp: 1, properties: FINESSE | THROWN, versatile_die: 0, range_normal: 20, range_long: 60, mastery: Mastery::Vex },
    WeaponDef { name: "Shortbow", category: WeaponCategory::Simple, cost_cp: 2500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: AMMUNITION | TWO_HANDED, versatile_die: 0, range_normal: 80, range_long: 320, mastery: Mastery::Vex },
    WeaponDef { name: "Sling", category: WeaponCategory::Simple, cost_cp: 10, damage_dice: 1, damage_die: 4, damage_type: DamageType::Bludgeoning, weight_qp: 0, properties: AMMUNITION, versatile_die: 0, range_normal: 30, range_long: 120, mastery: Mastery::Slow },
    // === Martial Melee ===
    WeaponDef { name: "Battleaxe", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Slashing, weight_qp: 16, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0, mastery: Mastery::Topple },
    WeaponDef { name: "Flail", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 8, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Sap },
    WeaponDef { name: "Glaive", category: WeaponCategory::Martial, cost_cp: 2000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Graze },
    WeaponDef { name: "Greataxe", category: WeaponCategory::Martial, cost_cp: 3000, damage_dice: 1, damage_die: 12, damage_type: DamageType::Slashing, weight_qp: 28, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Cleave },
    WeaponDef { name: "Greatsword", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 2, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Graze },
    WeaponDef { name: "Halberd", category: WeaponCategory::Martial, cost_cp: 2000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Slashing, weight_qp: 24, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Cleave },
    WeaponDef { name: "Lance", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 12, damage_type: DamageType::Piercing, weight_qp: 24, properties: REACH | SPECIAL, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Topple },
    WeaponDef { name: "Longsword", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Slashing, weight_qp: 12, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0, mastery: Mastery::Sap },
    WeaponDef { name: "Maul", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 2, damage_die: 6, damage_type: DamageType::Bludgeoning, weight_qp: 40, properties: HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Topple },
    WeaponDef { name: "Morningstar", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 16, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Sap },
    WeaponDef { name: "Pike", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 10, damage_type: DamageType::Piercing, weight_qp: 72, properties: HEAVY | REACH | TWO_HANDED, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Push },
    WeaponDef { name: "Rapier", category: WeaponCategory::Martial, cost_cp: 2500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: FINESSE, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Vex },
    WeaponDef { name: "Scimitar", category: WeaponCategory::Martial, cost_cp: 2500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Slashing, weight_qp: 12, properties: FINESSE | LIGHT, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Nick },
    WeaponDef { name: "Shortsword", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 8, properties: FINESSE | LIGHT, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Vex },
    WeaponDef { name: "Trident", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 16, properties: THROWN | VERSATILE, versatile_die: 8, range_normal: 20, range_long: 60, mastery: Mastery::Topple },
    WeaponDef { name: "War Pick", category: WeaponCategory::Martial, cost_cp: 500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: 0, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Sap },
    WeaponDef { name: "Warhammer", category: WeaponCategory::Martial, cost_cp: 1500, damage_dice: 1, damage_die: 8, damage_type: DamageType::Bludgeoning, weight_qp: 20, properties: VERSATILE, versatile_die: 10, range_normal: 0, range_long: 0, mastery: Mastery::Push },
    WeaponDef { name: "Whip", category: WeaponCategory::Martial, cost_cp: 200, damage_dice: 1, damage_die: 4, damage_type: DamageType::Slashing, weight_qp: 12, properties: FINESSE | REACH, versatile_die: 0, range_normal: 0, range_long: 0, mastery: Mastery::Slow },
    // === Martial Ranged ===
    WeaponDef { name: "Blowgun", category: WeaponCategory::Martial, cost_cp: 1000, damage_dice: 1, damage_die: 1, damage_type: DamageType::Piercing, weight_qp: 4, properties: AMMUNITION | LOADING, versatile_die: 0, range_normal: 25, range_long: 100, mastery: Mastery::Vex },
    WeaponDef { name: "Hand Crossbow", category: WeaponCategory::Martial, cost_cp: 7500, damage_dice: 1, damage_die: 6, damage_type: DamageType::Piercing, weight_qp: 12, properties: AMMUNITION | LIGHT | LOADING, versatile_die: 0, range_normal: 30, range_long: 120, mastery: Mastery::Vex },
    WeaponDef { name: "Heavy Crossbow", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Piercing, weight_qp: 72, properties: AMMUNITION | HEAVY | LOADING | TWO_HANDED, versatile_die: 0, range_normal: 100, range_long: 400, mastery: Mastery::Push },
    WeaponDef { name: "Longbow", category: WeaponCategory::Martial, cost_cp: 5000, damage_dice: 1, damage_die: 8, damage_type: DamageType::Piercing, weight_qp: 8, properties: AMMUNITION | HEAVY | TWO_HANDED, versatile_die: 0, range_normal: 150, range_long: 600, mastery: Mastery::Slow },
    WeaponDef { name: "Musket", category: WeaponCategory::Martial, cost_cp: 50000, damage_dice: 1, damage_die: 12, damage_type: DamageType::Piercing, weight_qp: 40, properties: AMMUNITION | LOADING | TWO_HANDED, versatile_die: 0, range_normal: 40, range_long: 120, mastery: Mastery::Slow },
    WeaponDef { name: "Pistol", category: WeaponCategory::Martial, cost_cp: 25000, damage_dice: 1, damage_die: 10, damage_type: DamageType::Piercing, weight_qp: 12, properties: AMMUNITION | LOADING, versatile_die: 0, range_normal: 30, range_long: 90, mastery: Mastery::Vex },
    // Net is a SPECIAL weapon with no SRD-listed mastery (it never deals
    // damage, so mastery hooks never fire). Filler Slow keeps the schema
    // uniform; see the mastery tests for rationale.
    WeaponDef { name: "Net", category: WeaponCategory::Martial, cost_cp: 100, damage_dice: 0, damage_die: 0, damage_type: DamageType::Bludgeoning, weight_qp: 12, properties: SPECIAL | THROWN, versatile_die: 0, range_normal: 5, range_long: 15, mastery: Mastery::Slow },
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

/// An SRD adventuring gear item definition (static data).
pub struct GearDef {
    pub name: &'static str,
    pub weight_qp: u32,  // quarter-pounds
    pub cost_cp: u32,    // copper pieces
    pub description: &'static str,
}

/// SRD adventuring gear table (22 items, PHB p. 148–153 / 2024 SRD §Equipment).
/// Weight encoded as quarter-pounds (1 lb = 4 qp). Cost in copper pieces.
pub const SRD_GEAR: &[GearDef] = &[
    GearDef { name: "Rope (50 ft)",          weight_qp: 40,  cost_cp: 100,   description: "Fifty feet of hempen rope. Can bear up to 600 lb." },
    GearDef { name: "Torch",                 weight_qp: 4,   cost_cp: 1,     description: "Burns for 1 hour, shedding bright light 20 ft and dim light 40 ft." },
    GearDef { name: "Bullseye Lantern",      weight_qp: 8,   cost_cp: 1000,  description: "Sheds bright light 60 ft and dim light 120 ft in a cone; burns 6 hours per flask." },
    GearDef { name: "Hooded Lantern",        weight_qp: 8,   cost_cp: 500,   description: "Sheds bright light 30 ft and dim light 60 ft; hood can cover the light." },
    GearDef { name: "Tinderbox",             weight_qp: 4,   cost_cp: 50,    description: "Contains flint, steel, and tinder for lighting fires." },
    GearDef { name: "Bedroll",               weight_qp: 28,  cost_cp: 100,   description: "A padded roll for sleeping." },
    GearDef { name: "Rations (1 day)",       weight_qp: 8,   cost_cp: 50,    description: "Dried food sufficient for one day." },
    GearDef { name: "Waterskin",             weight_qp: 20,  cost_cp: 20,    description: "Holds 4 pints of liquid; weighs 5 lb when full." },
    GearDef { name: "Crowbar",               weight_qp: 20,  cost_cp: 200,   description: "Grants advantage on STR checks where leverage applies." },
    GearDef { name: "Caltrops (bag)",        weight_qp: 8,   cost_cp: 100,   description: "Bag of 20 caltrops. Scattered over 5 ft; DC 15 DEX to avoid 1 piercing." },
    GearDef { name: "Ball Bearings (bag)",   weight_qp: 8,   cost_cp: 100,   description: "Bag of 1,000 ball bearings. Scattered over 10 ft; DC 10 DEX to avoid falling." },
    GearDef { name: "Hammer",               weight_qp: 4,   cost_cp: 100,   description: "A small iron hammer for driving pitons and stakes." },
    GearDef { name: "Piton",                 weight_qp: 1,   cost_cp: 5,     description: "An iron spike driven into stone to create a handhold." },
    GearDef { name: "Backpack",              weight_qp: 20,  cost_cp: 200,   description: "Can hold 1 cubic foot or 30 lb of gear." },
    GearDef { name: "Pouch",                 weight_qp: 4,   cost_cp: 50,    description: "A small leather pouch. Holds 6 lb or 1/5 cubic foot." },
    GearDef { name: "Sack",                  weight_qp: 4,   cost_cp: 1,     description: "A cloth sack. Holds 15 lb or 1 cubic foot." },
    GearDef { name: "Component Pouch",       weight_qp: 8,   cost_cp: 2500,  description: "A belt pouch containing spell components; replaces material components without a cost." },
    GearDef { name: "Spellbook",             weight_qp: 12,  cost_cp: 5000,  description: "A leather-bound tome holding up to 100 wizard spells." },
    GearDef { name: "Arcane Focus",          weight_qp: 4,   cost_cp: 1000,  description: "A crystal, orb, rod, staff, or wand for arcane spellcasting." },
    GearDef { name: "Holy Symbol",           weight_qp: 4,   cost_cp: 500,   description: "An amulet, emblem, or reliquary for divine spellcasting." },
    GearDef { name: "Druidic Focus",         weight_qp: 4,   cost_cp: 100,   description: "A sprig of mistletoe, totem, staff, or yew wand for druidic spellcasting." },
    GearDef { name: "Spyglass",              weight_qp: 4,   cost_cp: 100000, description: "Objects viewed through it appear twice as large." },
    GearDef { name: "Grappling Hook",        weight_qp: 16,  cost_cp: 200,   description: "A four-pronged hook for climbing with rope." },
];

/// Look up a gear item by its canonical SRD name (case-sensitive).
pub fn find_gear(name: &str) -> Option<&'static GearDef> {
    SRD_GEAR.iter().find(|g| g.name == name)
}


/// Returns `None` when the name is not present in `SRD_WEAPONS`. Name match
/// is case-sensitive, consistent with the rest of the SRD lookup API
/// (`SRD_WEAPONS.iter().find(|w| w.name == name)`).
pub fn weapon_mastery(name: &str) -> Option<Mastery> {
    SRD_WEAPONS.iter().find(|w| w.name == name).map(|w| w.mastery)
}

/// True when the character has unlocked Weapon Mastery for the named weapon.
/// Match is case-sensitive against `Character.weapon_masteries` entries,
/// which store the canonical SRD weapon name.
pub fn character_has_mastery(character: &Character, weapon_name: &str) -> bool {
    character.weapon_masteries.iter().any(|n| n == weapon_name)
}

pub fn calculate_ac(character: &Character, items: &HashMap<ItemId, Item>) -> i32 {
    let dex_mod = character.ability_modifier(Ability::Dexterity);

    // Body slot: mundane Armor or MagicArmor both contribute. MagicArmor adds
    // its `ac_bonus` on top of the base. `requires_attunement`-gated magic
    // armor only grants the bonus while attuned; base armor mechanics still
    // apply regardless (you're wearing it).
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
                Some(ItemType::MagicArmor { base_ac, max_dex_bonus, ac_bonus, requires_attunement, .. }) => {
                    let dex_contribution = match max_dex_bonus {
                        None => dex_mod,
                        Some(0) => 0,
                        Some(cap) => dex_mod.min(*cap as i32),
                    };
                    let bonus_applies = !requires_attunement
                        || character.attuned_items.contains(&body_id);
                    let bonus = if bonus_applies { *ac_bonus } else { 0 };
                    *base_ac as i32 + dex_contribution + bonus
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

    let wondrous_bonus = magic::wondrous_ac_bonus(character, items);

    base_ac + shield_bonus + wondrous_bonus
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
            charges_remaining: None,
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
        assert_eq!(SRD_WEAPONS.len(), 39);
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

    // Hypothesis: SRD_WEAPONS is missing the two Martial Ranged firearms
    // (Musket and Pistol) from the SRD reference (see
    // docs/reference/equipment.md:182-183). Appending both WeaponDef entries
    // to the Martial Ranged section should make these lookups succeed and
    // preserve the SRD stats documented in the handoff for #52.
    #[test]
    fn test_firearms_present_in_srd_weapons() {
        let musket = SRD_WEAPONS.iter().find(|w| w.name == "Musket")
            .expect("Musket must be present in SRD_WEAPONS");
        assert_eq!(musket.category, WeaponCategory::Martial);
        assert_eq!(musket.damage_dice, 1);
        assert_eq!(musket.damage_die, 12);
        assert_eq!(musket.damage_type, DamageType::Piercing);
        assert_eq!(musket.weight_qp, 40);
        assert_eq!(musket.cost_cp, 50000);
        assert_eq!(musket.range_normal, 40);
        assert_eq!(musket.range_long, 120);
        assert!(musket.properties & AMMUNITION != 0);
        assert!(musket.properties & LOADING != 0);
        assert!(musket.properties & TWO_HANDED != 0);

        let pistol = SRD_WEAPONS.iter().find(|w| w.name == "Pistol")
            .expect("Pistol must be present in SRD_WEAPONS");
        assert_eq!(pistol.category, WeaponCategory::Martial);
        assert_eq!(pistol.damage_dice, 1);
        assert_eq!(pistol.damage_die, 10);
        assert_eq!(pistol.damage_type, DamageType::Piercing);
        assert_eq!(pistol.weight_qp, 12);
        assert_eq!(pistol.cost_cp, 25000);
        assert_eq!(pistol.range_normal, 30);
        assert_eq!(pistol.range_long, 90);
        assert!(pistol.properties & AMMUNITION != 0);
        assert!(pistol.properties & LOADING != 0);
        assert!(pistol.properties & TWO_HANDED == 0);
    }

    // Hypothesis: The Warhammer entry in SRD_WEAPONS has weight_qp: 8
    // (= 2 lb), but the 2024 SRD specifies Warhammer weighs 5 lb. Weight is
    // stored in quarter-pounds (1 lb = 4 qp, see docs/specs/equipment-system.md),
    // so the correct value is 5 lb × 4 qp/lb = 20 qp. Updating the Warhammer
    // WeaponDef at jurnalis-engine/src/equipment/mod.rs:92 from weight_qp: 8
    // to weight_qp: 20 should make this assertion pass (see issue #53).
    #[test]
    fn test_warhammer_weight_matches_srd() {
        let warhammer = SRD_WEAPONS.iter().find(|w| w.name == "Warhammer")
            .expect("Warhammer must be present in SRD_WEAPONS");
        // SRD: Warhammer weighs 5 lb. Encoded as quarter-pounds: 5 * 4 = 20.
        assert_eq!(warhammer.weight_qp, 20, "Warhammer weight must be 5 lb (20 qp) per 2024 SRD");
        // Sanity-check other canonical stats so a regression on this entry
        // is caught holistically.
        assert_eq!(warhammer.category, WeaponCategory::Martial);
        assert_eq!(warhammer.damage_dice, 1);
        assert_eq!(warhammer.damage_die, 8);
        assert_eq!(warhammer.damage_type, DamageType::Bludgeoning);
        assert!(warhammer.properties & VERSATILE != 0);
        assert_eq!(warhammer.versatile_die, 10);
        assert_eq!(warhammer.cost_cp, 1500);
    }

    #[test]
    fn test_equipment_default_empty() {
        let eq = Equipment::default();
        assert!(eq.main_hand.is_none());
        assert!(eq.off_hand.is_none());
        assert!(eq.body.is_none());
    }

    // ---- Magic armor / wondrous AC bonus ----

    fn make_magic_armor(id: u32, name: &str, base_armor: &str, category: ArmorCategory, base_ac: u32, max_dex: Option<u32>, ac_bonus: i32) -> Item {
        use crate::equipment::magic::Rarity;
        Item {
            id, name: name.to_string(), description: String::new(),
            item_type: ItemType::MagicArmor {
                base_armor: base_armor.to_string(),
                category, base_ac, max_dex_bonus: max_dex,
                str_requirement: 0, stealth_disadvantage: false,
                ac_bonus,
                rarity: Rarity::Rare,
                requires_attunement: false,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        }
    }

    fn make_wondrous(id: u32, name: &str, effect: crate::equipment::magic::WondrousEffect, requires_attunement: bool) -> Item {
        use crate::equipment::magic::Rarity;
        Item {
            id, name: name.to_string(), description: String::new(),
            item_type: ItemType::Wondrous {
                effect,
                rarity: Rarity::Uncommon,
                requires_attunement,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        }
    }

    #[test]
    fn test_ac_magic_armor_adds_bonus() {
        let mut c = test_character(14); // DEX mod +2
        let mut items = HashMap::new();
        items.insert(0, make_magic_armor(0, "+2 Chain Mail", "Chain Mail", ArmorCategory::Heavy, 16, Some(0), 2));
        c.equipped.body = Some(0);
        // Base 16 heavy (no dex) + 2 bonus = 18
        assert_eq!(calculate_ac(&c, &items), 18);
    }

    #[test]
    fn test_ac_wondrous_cloak_of_protection_when_attuned() {
        use crate::equipment::magic::WondrousEffect;
        let mut c = test_character(14); // DEX mod +2
        let mut items = HashMap::new();
        items.insert(5, make_wondrous(5, "Cloak of Protection", WondrousEffect::CloakOfProtection, true));
        c.inventory.push(5);
        // Not attuned yet — no bonus.
        assert_eq!(calculate_ac(&c, &items), 12); // 10 + 2 dex
        // Attuned — +1 AC.
        c.attuned_items.push(5);
        assert_eq!(calculate_ac(&c, &items), 13);
    }

    #[test]
    fn test_ac_wondrous_ring_of_protection_stacks_with_cloak() {
        use crate::equipment::magic::WondrousEffect;
        let mut c = test_character(14); // DEX mod +2
        let mut items = HashMap::new();
        items.insert(5, make_wondrous(5, "Cloak of Protection", WondrousEffect::CloakOfProtection, true));
        items.insert(6, make_wondrous(6, "Ring of Protection", WondrousEffect::RingOfProtection, true));
        c.inventory.push(5);
        c.inventory.push(6);
        c.attuned_items.push(5);
        c.attuned_items.push(6);
        // 10 + 2 dex + 1 cloak + 1 ring = 14
        assert_eq!(calculate_ac(&c, &items), 14);
    }

    #[test]
    fn test_ac_wondrous_requires_attunement_when_flagged() {
        use crate::equipment::magic::WondrousEffect;
        let mut c = test_character(14);
        let mut items = HashMap::new();
        // Flag as not requiring attunement — bonus should apply without attuning.
        items.insert(5, make_wondrous(5, "Cloak of Protection", WondrousEffect::CloakOfProtection, false));
        // Even without attuning, if requires_attunement is false the bonus applies
        // (while item is in inventory).
        // For MVP, bonus only applies while attuned OR item doesn't require attunement
        // AND the item is in the character inventory.
        c.inventory.push(5);
        assert_eq!(calculate_ac(&c, &items), 13);
    }

    // ---- Weapon Mastery (2024 SRD, feat/weapon-mastery) ----

    #[test]
    fn test_srd_weapons_have_mastery_populated() {
        // Every SRD weapon must have a mastery. This spot-checks a handful
        // per the 2024 SRD reference table (docs/reference/equipment.md).
        use crate::types::Mastery;
        let find = |name: &str| SRD_WEAPONS.iter().find(|w| w.name == name)
            .unwrap_or_else(|| panic!("{} missing from SRD_WEAPONS", name));

        // Simple Melee
        assert_eq!(find("Club").mastery, Mastery::Slow);
        assert_eq!(find("Dagger").mastery, Mastery::Nick);
        assert_eq!(find("Greatclub").mastery, Mastery::Push);
        assert_eq!(find("Handaxe").mastery, Mastery::Vex);
        assert_eq!(find("Javelin").mastery, Mastery::Slow);
        assert_eq!(find("Light Hammer").mastery, Mastery::Nick);
        assert_eq!(find("Mace").mastery, Mastery::Sap);
        assert_eq!(find("Quarterstaff").mastery, Mastery::Topple);
        assert_eq!(find("Sickle").mastery, Mastery::Nick);
        assert_eq!(find("Spear").mastery, Mastery::Sap);
        // Simple Ranged
        assert_eq!(find("Dart").mastery, Mastery::Vex);
        assert_eq!(find("Light Crossbow").mastery, Mastery::Slow);
        assert_eq!(find("Shortbow").mastery, Mastery::Vex);
        assert_eq!(find("Sling").mastery, Mastery::Slow);
        // Martial Melee
        assert_eq!(find("Battleaxe").mastery, Mastery::Topple);
        assert_eq!(find("Flail").mastery, Mastery::Sap);
        assert_eq!(find("Glaive").mastery, Mastery::Graze);
        assert_eq!(find("Greataxe").mastery, Mastery::Cleave);
        assert_eq!(find("Greatsword").mastery, Mastery::Graze);
        assert_eq!(find("Halberd").mastery, Mastery::Cleave);
        assert_eq!(find("Lance").mastery, Mastery::Topple);
        assert_eq!(find("Longsword").mastery, Mastery::Sap);
        assert_eq!(find("Maul").mastery, Mastery::Topple);
        assert_eq!(find("Morningstar").mastery, Mastery::Sap);
        assert_eq!(find("Pike").mastery, Mastery::Push);
        assert_eq!(find("Rapier").mastery, Mastery::Vex);
        assert_eq!(find("Scimitar").mastery, Mastery::Nick);
        assert_eq!(find("Shortsword").mastery, Mastery::Vex);
        assert_eq!(find("Trident").mastery, Mastery::Topple);
        assert_eq!(find("War Pick").mastery, Mastery::Sap);
        assert_eq!(find("Warhammer").mastery, Mastery::Push);
        assert_eq!(find("Whip").mastery, Mastery::Slow);
        // Martial Ranged
        assert_eq!(find("Blowgun").mastery, Mastery::Vex);
        assert_eq!(find("Hand Crossbow").mastery, Mastery::Vex);
        assert_eq!(find("Heavy Crossbow").mastery, Mastery::Push);
        assert_eq!(find("Longbow").mastery, Mastery::Slow);
        assert_eq!(find("Musket").mastery, Mastery::Slow);
        assert_eq!(find("Pistol").mastery, Mastery::Vex);
        // Net is a SPECIAL weapon. The SRD mastery table does not enumerate
        // it because Net has no dice damage, but the engine still requires
        // every WeaponDef to carry a mastery. We assign Slow as a harmless
        // filler — the combat hooks check mastery unlock + damage dealt,
        // and Net never deals damage, so Slow never fires.
        assert_eq!(find("Net").mastery, Mastery::Slow);
    }

    #[test]
    fn test_weapon_mastery_lookup_by_name() {
        use crate::types::Mastery;
        assert_eq!(weapon_mastery("Longsword"), Some(Mastery::Sap));
        assert_eq!(weapon_mastery("Greataxe"), Some(Mastery::Cleave));
        assert_eq!(weapon_mastery("Shortsword"), Some(Mastery::Vex));
        assert_eq!(weapon_mastery("Dagger"), Some(Mastery::Nick));
        // Case-sensitive canonical match, per SRD lookup conventions.
        assert_eq!(weapon_mastery("longsword"), None);
        assert_eq!(weapon_mastery("Spork"), None);
    }

    #[test]
    fn test_character_has_mastery_checks_vec() {
        let mut c = test_character(14);
        // Reset the auto-filled mastery list so this test exercises the
        // predicate in isolation from create_character's starting-loadout
        // initialization.
        c.weapon_masteries.clear();
        assert!(!character_has_mastery(&c, "Longsword"));
        c.weapon_masteries.push("Longsword".to_string());
        assert!(character_has_mastery(&c, "Longsword"));
        // Only exact matches count.
        assert!(!character_has_mastery(&c, "longsword"));
        assert!(!character_has_mastery(&c, "Shortsword"));
    }

    #[test]
    fn test_ac_magic_armor_medium_respects_dex_cap() {
        let mut c = test_character(18); // DEX mod +4
        let mut items = HashMap::new();
        items.insert(0, make_magic_armor(0, "+1 Breastplate", "Breastplate", ArmorCategory::Medium, 14, Some(2), 1));
        c.equipped.body = Some(0);
        // 14 + min(4, 2) + 1 bonus = 17
        assert_eq!(calculate_ac(&c, &items), 17);
    }

    #[test]
    fn test_srd_gear_count() {
        assert_eq!(SRD_GEAR.len(), 23, "SRD_GEAR should have 23 entries");
    }

    #[test]
    fn test_find_gear_returns_known_item() {
        let g = find_gear("Rope (50 ft)").expect("Rope should exist in SRD_GEAR");
        assert_eq!(g.name, "Rope (50 ft)");
        assert_eq!(g.weight_qp, 40);
        assert_eq!(g.cost_cp, 100);
    }

    #[test]
    fn test_find_gear_returns_none_for_unknown() {
        assert!(find_gear("Nonexistent Item").is_none());
    }

    #[test]
    fn test_gear_names_are_unique() {
        let mut names = std::collections::HashSet::new();
        for g in SRD_GEAR {
            assert!(names.insert(g.name), "Duplicate gear name: {}", g.name);
        }
    }
}
