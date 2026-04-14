// jurnalis-engine/src/world/mod.rs
pub mod location;
pub mod npc;
pub mod item;
pub mod trigger;

use rand::Rng;
use crate::state::{WorldState, Disposition};
use crate::combat::monsters;
use std::collections::{HashMap, HashSet};

pub fn generate_world(rng: &mut impl Rng, location_count: usize) -> WorldState {
    let mut locations = location::generate_locations(rng, location_count);
    let location_refs: HashMap<_, _> = locations.iter().map(|(k, v)| (*k, v)).collect();
    let location_ids: Vec<_> = locations.keys().copied().collect();

    let npc_count = location_count / 3 + 1;
    let mut npcs = npc::generate_npcs(rng, &location_refs, npc_count);

    // Assign combat stats to hostile NPCs from the SRD monster table, biased
    // by the NPC's location index (depth proxy). See `docs/specs/world-generation.md`
    // for the tier windows. Ensures first-encounter enemies are survivable at
    // level 1 while deeper rooms escalate in difficulty.
    for npc in npcs.values_mut() {
        if npc.disposition == Disposition::Hostile {
            let depth = npc.location as usize;
            let def = monsters::select_monster_for_depth(rng, depth);
            npc.combat_stats = Some(monsters::monster_to_combat_stats(def));
        }
    }

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
    fn test_all_hostile_npcs_have_combat_stats() {
        // Regression test: every hostile NPC must have combat_stats assigned.
        // Previously find_monster() could silently return None, leaving
        // hostile NPCs without stats ("ghost NPCs").
        let mut rng = StdRng::seed_from_u64(42);
        let world = generate_world(&mut rng, 15);
        for npc in world.npcs.values() {
            if npc.disposition == crate::state::Disposition::Hostile {
                assert!(npc.combat_stats.is_some(),
                    "Hostile NPC '{}' (id={}) must have combat_stats", npc.name, npc.id);
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

    #[test]
    fn test_hostile_hp_scales_with_location_depth() {
        // Every hostile NPC's combat stats must fall within the HP window
        // implied by its location index (depth proxy).
        for seed in 0..16u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let world = generate_world(&mut rng, 20);
            for npc in world.npcs.values() {
                if npc.disposition != crate::state::Disposition::Hostile {
                    continue;
                }
                let stats = npc.combat_stats.as_ref()
                    .expect("hostile NPC must have combat_stats");
                let depth = npc.location as usize;
                let (lo, hi) = match depth {
                    0..=3 => (5, 12),
                    4..=8 => (10, 18),
                    _ => (15, 25),
                };
                assert!(
                    stats.max_hp >= lo && stats.max_hp <= hi,
                    "seed {}: hostile '{}' at location {} (depth tier lo={},hi={}) has max_hp {}",
                    seed, npc.name, npc.location, lo, hi, stats.max_hp
                );
            }
        }
    }

    #[test]
    fn test_shallow_hostiles_have_survivable_hp() {
        // Regression: first-encounter enemies (location 0-3) must cap at 12 HP.
        // Previously, unweighted SRD sampling could spawn 22+ HP monsters in
        // room 0, making level-1 play unwinnable.
        for seed in 0..64u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let world = generate_world(&mut rng, 15);
            for npc in world.npcs.values() {
                if npc.disposition != crate::state::Disposition::Hostile {
                    continue;
                }
                if (npc.location as usize) <= 3 {
                    let hp = npc.combat_stats.as_ref().unwrap().max_hp;
                    assert!(
                        hp <= 12,
                        "seed {}: shallow hostile '{}' at loc {} has max_hp {} (>12)",
                        seed, npc.name, npc.location, hp
                    );
                }
            }
        }
    }
}
