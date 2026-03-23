// jurnalis-engine/src/world/mod.rs
pub mod location;
pub mod npc;
pub mod item;
pub mod trigger;

use rand::Rng;
use crate::state::WorldState;
use std::collections::{HashMap, HashSet};

pub fn generate_world(rng: &mut impl Rng, location_count: usize) -> WorldState {
    let mut locations = location::generate_locations(rng, location_count);
    let location_refs: HashMap<_, _> = locations.iter().map(|(k, v)| (*k, v)).collect();
    let location_ids: Vec<_> = locations.keys().copied().collect();

    let npc_count = location_count / 3 + 1;
    let npcs = npc::generate_npcs(rng, &location_refs, npc_count);

    let item_count = location_count / 2 + 2;
    let items = item::generate_items(rng, &location_ids, item_count);

    let trigger_count = location_count / 3 + 1;
    let triggers = trigger::generate_triggers(rng, &location_ids, trigger_count);

    // Link NPCs, items, and triggers back to their locations
    for npc in npcs.values() {
        if let Some(loc) = locations.get_mut(&npc.location) {
            loc.npcs.push(npc.id);
        }
    }
    for item in items.values() {
        if let Some(loc_id) = item.location {
            if let Some(loc) = locations.get_mut(&loc_id) {
                loc.items.push(item.id);
            }
        }
    }
    for trigger in triggers.values() {
        if let Some(loc) = locations.get_mut(&trigger.location) {
            loc.triggers.push(trigger.id);
        }
    }

    WorldState {
        locations,
        npcs,
        items,
        triggers,
        triggered: HashSet::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_generate_world() {
        let mut rng = StdRng::seed_from_u64(42);
        let world = generate_world(&mut rng, 15);
        assert_eq!(world.locations.len(), 15);
        assert!(!world.npcs.is_empty());
        assert!(!world.items.is_empty());
        assert!(!world.triggers.is_empty());
    }

    #[test]
    fn test_npcs_linked_to_locations() {
        let mut rng = StdRng::seed_from_u64(42);
        let world = generate_world(&mut rng, 10);
        for npc in world.npcs.values() {
            let loc = &world.locations[&npc.location];
            assert!(loc.npcs.contains(&npc.id), "NPC {} not linked to location {}", npc.id, loc.id);
        }
    }

    #[test]
    fn test_items_linked_to_locations() {
        let mut rng = StdRng::seed_from_u64(42);
        let world = generate_world(&mut rng, 10);
        for item in world.items.values() {
            if let Some(loc_id) = item.location {
                let loc = &world.locations[&loc_id];
                assert!(loc.items.contains(&item.id));
            }
        }
    }

    #[test]
    fn test_deterministic_world() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let w1 = generate_world(&mut rng1, 10);
        let w2 = generate_world(&mut rng2, 10);
        assert_eq!(w1.npcs.len(), w2.npcs.len());
        assert_eq!(w1.items.len(), w2.items.len());
        assert_eq!(w1.triggers.len(), w2.triggers.len());
    }
}
