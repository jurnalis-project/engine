// jurnalis-engine/src/world/item.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{ItemId, LocationId};
use crate::state::{Item, ItemType, DamageType, WeaponCategory};

const WEAPONS: &[(&str, &str, u32)] = &[
    ("Rusty Shortsword", "A battered shortsword, still sharp enough to cut.", 6),
    ("Wooden Club", "A simple club carved from oak.", 4),
    ("Dagger", "A small blade, easy to conceal.", 4),
    ("Handaxe", "A sturdy axe suited for one hand.", 6),
    ("Quarterstaff", "A long wooden staff, balanced for fighting.", 6),
];

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

        let item = match rng.gen_range(0..3) {
            0 => {
                let w = &WEAPONS[rng.gen_range(0..WEAPONS.len())];
                Item {
                    id,
                    name: w.0.to_string(),
                    description: w.1.to_string(),
                    item_type: ItemType::Weapon {
                        damage_dice: 1,
                        damage_die: w.2,
                        damage_type: DamageType::Slashing,
                        properties: 0,
                        category: WeaponCategory::Simple,
                        versatile_die: 0,
                        range_normal: 0,
                        range_long: 0,
                    },
                    location: Some(location),
                    carried_by_player: false,
                }
            }
            1 => {
                let c = &CONSUMABLES[rng.gen_range(0..CONSUMABLES.len())];
                Item {
                    id,
                    name: c.0.to_string(),
                    description: c.1.to_string(),
                    item_type: ItemType::Consumable { effect: c.2.to_string() },
                    location: Some(location),
                    carried_by_player: false,
                }
            }
            _ => {
                let m = &MISC_ITEMS[rng.gen_range(0..MISC_ITEMS.len())];
                Item {
                    id,
                    name: m.0.to_string(),
                    description: m.1.to_string(),
                    item_type: ItemType::Misc,
                    location: Some(location),
                    carried_by_player: false,
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
}
