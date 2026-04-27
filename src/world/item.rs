// jurnalis-engine/src/world/item.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{ItemId, LocationId};
use crate::state::{Item, ItemType, WeaponCategory};
use crate::equipment::{SRD_WEAPONS, SRD_ARMOR, SRD_GEAR};
use crate::equipment::magic::{
    Rarity, MagicItemDef, MagicItemKind,
    SRD_MAGIC_WEAPONS, SRD_MAGIC_ARMOR, SRD_WONDROUS,
    SRD_POTIONS, SRD_SCROLLS, SRD_WANDS,
};

/// Probability that any given loot roll spawns a magic item (otherwise
/// falls through to the mundane weapon/armor/consumable/misc tables).
const MAGIC_SPAWN_CHANCE: f64 = 0.05;

/// Rarity weights for magic-item spawns. Derived from the handoff scope:
/// Common 50, Uncommon 30, Rare 15, VeryRare 4, Legendary 1.
/// Artifact is intentionally excluded from random world loot.
const RARITY_WEIGHTS: &[(Rarity, u32)] = &[
    (Rarity::Common,    50),
    (Rarity::Uncommon,  30),
    (Rarity::Rare,      15),
    (Rarity::VeryRare,   4),
    (Rarity::Legendary,  1),
];

/// Pick a rarity according to the weighted table above.
fn pick_rarity(rng: &mut impl Rng) -> Rarity {
    let total: u32 = RARITY_WEIGHTS.iter().map(|(_, w)| *w).sum();
    let mut r = rng.gen_range(0..total);
    for (rarity, w) in RARITY_WEIGHTS {
        if r < *w { return *rarity; }
        r -= *w;
    }
    Rarity::Common
}

/// Return a random `MagicItemDef` at the given rarity across all SRD tables.
/// Returns `None` if no magic item at that rarity exists.
fn pick_magic_item_def(rng: &mut impl Rng, rarity: Rarity) -> Option<&'static MagicItemDef> {
    let pool: Vec<&'static MagicItemDef> = SRD_MAGIC_WEAPONS.iter()
        .chain(SRD_MAGIC_ARMOR.iter())
        .chain(SRD_WONDROUS.iter())
        .chain(SRD_POTIONS.iter())
        .chain(SRD_SCROLLS.iter())
        .chain(SRD_WANDS.iter())
        .filter(|d| d.rarity == rarity)
        .collect();
    if pool.is_empty() {
        return None;
    }
    Some(pool[rng.gen_range(0..pool.len())])
}

/// Instantiate a world `Item` from a `MagicItemDef`. For weapons/armor this
/// pulls the base weapon/armor fields from SRD tables (so combat/AC
/// calculation can treat them uniformly with mundane items).
fn materialize_magic_item(
    id: ItemId,
    location: LocationId,
    def: &'static MagicItemDef,
) -> Item {
    match &def.kind {
        MagicItemKind::MagicWeapon { base_weapon, attack_bonus, damage_bonus } => {
            let w = SRD_WEAPONS.iter().find(|w| w.name == *base_weapon)
                .expect("magic weapon base_weapon must exist in SRD_WEAPONS");
            Item {
                id, name: def.name.to_string(),
                description: format!("A {}, humming with magic.", def.name.to_lowercase()),
                item_type: ItemType::MagicWeapon {
                    base_weapon: w.name.to_string(),
                    damage_dice: w.damage_dice, damage_die: w.damage_die,
                    damage_type: w.damage_type, properties: w.properties,
                    category: w.category, versatile_die: w.versatile_die,
                    range_normal: w.range_normal, range_long: w.range_long,
                    attack_bonus: *attack_bonus, damage_bonus: *damage_bonus,
                    rarity: def.rarity, requires_attunement: def.requires_attunement,
                },
                location: Some(location), carried_by_player: false,
                charges_remaining: None,
            }
        }
        MagicItemKind::MagicArmor { base_armor, ac_bonus } => {
            let a = SRD_ARMOR.iter().find(|a| a.name == *base_armor)
                .expect("magic armor base_armor must exist in SRD_ARMOR");
            Item {
                id, name: def.name.to_string(),
                description: format!("A {}, faintly radiant.", def.name.to_lowercase()),
                item_type: ItemType::MagicArmor {
                    base_armor: a.name.to_string(),
                    category: a.category,
                    base_ac: a.base_ac,
                    max_dex_bonus: a.max_dex_bonus,
                    str_requirement: a.str_requirement,
                    stealth_disadvantage: a.stealth_disadvantage,
                    ac_bonus: *ac_bonus,
                    rarity: def.rarity, requires_attunement: def.requires_attunement,
                },
                location: Some(location), carried_by_player: false,
                charges_remaining: None,
            }
        }
        MagicItemKind::Wondrous { effect } => {
            Item {
                id, name: def.name.to_string(),
                description: format!("A {}, alive with subtle enchantment.", def.name.to_lowercase()),
                item_type: ItemType::Wondrous {
                    effect: *effect,
                    rarity: def.rarity, requires_attunement: def.requires_attunement,
                },
                location: Some(location), carried_by_player: false,
                charges_remaining: None,
            }
        }
        MagicItemKind::Potion { effect } => {
            Item {
                id, name: def.name.to_string(),
                description: format!("A glass vial: {}.", def.name.to_lowercase()),
                item_type: ItemType::Potion { effect: *effect, rarity: def.rarity },
                location: Some(location), carried_by_player: false,
                charges_remaining: None,
            }
        }
        MagicItemKind::Scroll { spell_name, spell_level } => {
            Item {
                id, name: def.name.to_string(),
                description: format!("A brittle parchment inscribed with {}.", spell_name),
                item_type: ItemType::Scroll {
                    spell_name: spell_name.to_string(),
                    spell_level: *spell_level,
                    rarity: def.rarity,
                },
                location: Some(location), carried_by_player: false,
                charges_remaining: None,
            }
        }
        MagicItemKind::Wand { spell_name, charges_max } => {
            Item {
                id, name: def.name.to_string(),
                description: format!("A slender wand humming with {}-aligned magic.", spell_name),
                item_type: ItemType::Wand {
                    spell_name: spell_name.to_string(),
                    rarity: def.rarity,
                    requires_attunement: def.requires_attunement,
                },
                location: Some(location), carried_by_player: false,
                charges_remaining: Some(*charges_max),
            }
        }
    }
}

const CONSUMABLES: &[(&str, &str, &str)] = &[
    // Note: "Healing Potion" here matches SRD 2024 Potion of Healing (2d4 + 2).
    // The effect code "heal_srd_potion" is handled in lib.rs::resolve_use_item.
    ("Healing Potion", "A small vial of red liquid that restores vitality.", "heal_srd_potion"),
    ("Torch", "A wooden torch soaked in pitch. Provides light.", "light"),
    ("Rations", "A day's worth of dried food.", "nourish"),
];

const MISC_ITEMS: &[(&str, &str)] = &[
    ("Old Coin", "A tarnished coin from a forgotten kingdom."),
    ("Cracked Gemstone", "A once-valuable gem, now cracked."),
    ("Torn Map", "A fragment of a map showing unknown passages."),
    ("Iron Key", "A heavy iron key. It must unlock something."),
    ("Bone Amulet", "An amulet carved from bone, faintly warm."),
];

pub fn generate_items(
    rng: &mut impl Rng,
    location_ids: &[LocationId],
    item_count: usize,
) -> HashMap<ItemId, Item> {
    let mut items = HashMap::new();
    if location_ids.is_empty() {
        return items;
    }

    for i in 0..item_count {
        let id = i as ItemId;
        let location = location_ids[rng.gen_range(0..location_ids.len())];

        // Magic item spawn roll: small percentage of total loot is magical.
        // Rarity is weighted heavily toward Common/Uncommon so Legendary
        // items remain a rare treat (matches handoff scope).
        if rng.gen_bool(MAGIC_SPAWN_CHANCE) {
            let rarity = pick_rarity(rng);
            if let Some(def) = pick_magic_item_def(rng, rarity) {
                let item = materialize_magic_item(id, location, def);
                items.insert(id, item);
                continue;
            }
            // If the rarity pool is empty, fall through to mundane loot.
        }

        let item = match rng.gen_range(0..4) {
            0 => {
                // Weapon from SRD table — simple weapons 2x more likely
                let simple: Vec<_> = SRD_WEAPONS.iter().filter(|w| w.category == WeaponCategory::Simple).collect();
                let martial: Vec<_> = SRD_WEAPONS.iter().filter(|w| w.category == WeaponCategory::Martial).collect();
                let w = if rng.gen_bool(0.67) && !simple.is_empty() {
                    simple[rng.gen_range(0..simple.len())]
                } else {
                    let all = if martial.is_empty() { &simple } else { &martial };
                    all[rng.gen_range(0..all.len())]
                };
                Item {
                    id, name: w.name.to_string(),
                    description: format!("A {}.", w.name.to_lowercase()),
                    item_type: ItemType::Weapon {
                        damage_dice: w.damage_dice, damage_die: w.damage_die,
                        damage_type: w.damage_type, properties: w.properties,
                        category: w.category, versatile_die: w.versatile_die,
                        range_normal: w.range_normal, range_long: w.range_long,
                    },
                    location: Some(location), carried_by_player: false,
                    charges_remaining: None,
                }
            }
            1 => {
                // Armor from SRD table — light armor more common
                let light: Vec<_> = SRD_ARMOR.iter()
                    .filter(|a| matches!(a.category, crate::state::ArmorCategory::Light | crate::state::ArmorCategory::Shield))
                    .collect();
                let heavier: Vec<_> = SRD_ARMOR.iter()
                    .filter(|a| !matches!(a.category, crate::state::ArmorCategory::Light | crate::state::ArmorCategory::Shield))
                    .collect();
                let a = if rng.gen_bool(0.6) && !light.is_empty() {
                    light[rng.gen_range(0..light.len())]
                } else if !heavier.is_empty() {
                    heavier[rng.gen_range(0..heavier.len())]
                } else {
                    light[rng.gen_range(0..light.len())]
                };
                Item {
                    id, name: a.name.to_string(),
                    description: format!("A set of {} armor.", a.name.to_lowercase()),
                    item_type: ItemType::Armor {
                        category: a.category, base_ac: a.base_ac,
                        max_dex_bonus: a.max_dex_bonus, str_requirement: a.str_requirement,
                        stealth_disadvantage: a.stealth_disadvantage,
                    },
                    location: Some(location), carried_by_player: false,
                    charges_remaining: None,
                }
            }
            2 => {
                let c = &CONSUMABLES[rng.gen_range(0..CONSUMABLES.len())];
                Item {
                    id, name: c.0.to_string(), description: c.1.to_string(),
                    item_type: ItemType::Consumable { effect: c.2.to_string() },
                    location: Some(location), carried_by_player: false,
                    charges_remaining: None,
                }
            }
            _ => {
                // Slot 3: split between misc items and adventuring gear (50/50).
                if rng.gen_bool(0.5) {
                    let g = &SRD_GEAR[rng.gen_range(0..SRD_GEAR.len())];
                    Item {
                        id, name: g.name.to_string(), description: g.description.to_string(),
                        item_type: ItemType::GearItem {
                            gear_name: g.name.to_string(),
                            weight_qp: g.weight_qp,
                            cost_cp: g.cost_cp,
                        },
                        location: Some(location), carried_by_player: false,
                        charges_remaining: None,
                    }
                } else {
                    let m = &MISC_ITEMS[rng.gen_range(0..MISC_ITEMS.len())];
                    Item {
                        id, name: m.0.to_string(), description: m.1.to_string(),
                        item_type: ItemType::Misc,
                        location: Some(location), carried_by_player: false,
                        charges_remaining: None,
                    }
                }
            }
        };

        items.insert(id, item);
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_generates_correct_count() {
        let mut rng = StdRng::seed_from_u64(42);
        let items = generate_items(&mut rng, &[0, 1, 2], 10);
        assert_eq!(items.len(), 10);
    }

    #[test]
    fn test_items_placed_in_valid_locations() {
        let mut rng = StdRng::seed_from_u64(42);
        let loc_ids = vec![0, 1, 2, 3];
        let items = generate_items(&mut rng, &loc_ids, 10);
        for item in items.values() {
            assert!(loc_ids.contains(&item.location.unwrap()));
        }
    }

    #[test]
    fn test_items_not_carried() {
        let mut rng = StdRng::seed_from_u64(42);
        let items = generate_items(&mut rng, &[0], 5);
        for item in items.values() {
            assert!(!item.carried_by_player);
        }
    }

    #[test]
    fn test_generates_weapons_from_srd_table() {
        use crate::equipment::SRD_WEAPONS;
        let mut rng = StdRng::seed_from_u64(42);
        let items = generate_items(&mut rng, &[0, 1, 2], 20);
        let weapons: Vec<_> = items.values().filter(|i| matches!(i.item_type, ItemType::Weapon { .. })).collect();
        assert!(!weapons.is_empty(), "Should generate some weapons");
        // Verify weapon names come from SRD table
        for w in &weapons {
            assert!(SRD_WEAPONS.iter().any(|srd| srd.name == w.name),
                "Weapon '{}' should be from SRD table", w.name);
        }
    }

    #[test]
    fn test_generates_armor() {
        let mut rng = StdRng::seed_from_u64(42);
        let items = generate_items(&mut rng, &[0, 1, 2], 30);
        let armor: Vec<_> = items.values().filter(|i| matches!(i.item_type, ItemType::Armor { .. })).collect();
        assert!(!armor.is_empty(), "Should generate some armor with 30 items");
    }
}
