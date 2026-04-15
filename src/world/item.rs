// jurnalis-engine/src/world/item.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{ItemId, LocationId};
use crate::state::{Item, ItemType, WeaponCategory};
use crate::equipment::{SRD_WEAPONS, SRD_ARMOR};

const CONSUMABLES: &[(&str, &str, &str)] = &[
    ("Healing Potion", "A small vial of red liquid that restores vitality.", "heal_1d8"),
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
                let m = &MISC_ITEMS[rng.gen_range(0..MISC_ITEMS.len())];
                Item {
                    id, name: m.0.to_string(), description: m.1.to_string(),
                    item_type: ItemType::Misc,
                    location: Some(location), carried_by_player: false,
                    charges_remaining: None,
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
