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
    let mut location_ids: Vec<_> = locations.keys().copied().collect();
    location_ids.sort();

    let npc_count = location_count / 3 + 1;
    let mut npcs = npc::generate_npcs(rng, &location_refs, npc_count);

    // Guarantee at least one hostile NPC. Disposition assignment in
    // `npc::generate_npcs` is uniform random, so small worlds can land on
    // zero hostiles, which makes the starting "Defeat a hostile foe"
    // objective (seeded in `lib.rs` per `docs/specs/objective-quest-system.md`)
    // uncompletable. If we see zero hostiles, promote one NPC in place.
    // Preference order: Guard role (thematically hostile-adjacent), then the
    // lowest-id NPC. Selection is deterministic (no RNG draws here) so that
    // worlds which already had a hostile continue generating byte-for-byte
    // the same items/triggers as before this guarantee existed.
    if !npcs.is_empty()
        && !npcs.values().any(|n| n.disposition == Disposition::Hostile)
    {
        // Prefer a Guard (thematically hostile-adjacent). Fall back to the
        // lowest-id non-Merchant NPC. Merchants must never become hostile —
        // they are always peaceful and available for trade.
        let promote_id = npcs
            .values()
            .filter(|n| n.role == crate::state::NpcRole::Guard)
            .map(|n| n.id)
            .min()
            .or_else(|| {
                npcs.values()
                    .filter(|n| n.role != crate::state::NpcRole::Merchant)
                    .map(|n| n.id)
                    .min()
            });
        if let Some(id) = promote_id {
            if let Some(npc) = npcs.get_mut(&id) {
                npc.disposition = Disposition::Hostile;
            }
        }
    }

    // Assign combat stats to hostile NPCs from the SRD monster table, biased
    // by the NPC's location index (depth proxy). See `docs/specs/world-generation.md`
    // for the tier windows. Ensures first-encounter enemies are survivable at
    // level 1 while deeper rooms escalate in difficulty.
    //
    // Iterate by sorted NPC ID so the RNG draws are deterministic regardless
    // of HashMap iteration order.
    let mut npc_ids: Vec<_> = npcs.keys().copied().collect();
    npc_ids.sort();
    for npc_id in npc_ids {
        let npc = npcs.get_mut(&npc_id).unwrap();
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

    // Link NPCs, items, and triggers back to their locations.
    // Iterate by sorted ID for deterministic loc.npcs / loc.items / loc.triggers order.
    let mut sorted_npc_ids: Vec<_> = npcs.keys().copied().collect();
    sorted_npc_ids.sort();
    for id in sorted_npc_ids {
        let npc = &npcs[&id];
        if let Some(loc) = locations.get_mut(&npc.location) {
            loc.npcs.push(npc.id);
        }
    }
    let mut sorted_item_ids: Vec<_> = items.keys().copied().collect();
    sorted_item_ids.sort();
    for id in sorted_item_ids {
        let item = &items[&id];
        if let Some(loc_id) = item.location {
            if let Some(loc) = locations.get_mut(&loc_id) {
                loc.items.push(item.id);
            }
        }
    }
    let mut sorted_trigger_ids: Vec<_> = triggers.keys().copied().collect();
    sorted_trigger_ids.sort();
    for id in sorted_trigger_ids {
        let trigger = &triggers[&id];
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
                    _ => (15, 35), // extended to 35 to include Dark Mage (HP 32, CR 2)
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
    fn test_every_seed_produces_at_least_one_hostile_npc() {
        // The at-least-one-hostile guarantee promotes a non-Merchant NPC to
        // Hostile when no NPC was naturally assigned Hostile. Merchants are
        // excluded from promotion (they must always be peaceful). If every NPC
        // in the world happens to be a Merchant, zero hostiles is acceptable
        // because promoting a Merchant would break the trading contract.
        //
        // See: docs/specs/world-generation.md, docs/specs/objective-quest-system.md
        for seed in 0..2000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let world = generate_world(&mut rng, 10);
            let has_non_merchant = world.npcs.values()
                .any(|n| n.role != crate::state::NpcRole::Merchant);
            if !has_non_merchant {
                // All-Merchant world: zero hostiles is acceptable
                continue;
            }
            let hostile_count = world.npcs.values()
                .filter(|n| n.disposition == crate::state::Disposition::Hostile)
                .count();
            assert!(
                hostile_count >= 1,
                "seed {}: generated world has zero hostile NPCs (total npcs={}) \
                 despite having non-Merchant NPCs",
                seed, world.npcs.len()
            );
        }
    }

    #[test]
    fn test_force_converted_hostile_has_combat_stats() {
        // Any hostile NPC created by the post-gen guarantee must still have
        // combat_stats assigned (via the existing depth-tiered logic).
        // Exercises many seeds so that both the "already has a hostile" path
        // and the "force-convert" path are covered.
        for seed in 0..2000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let world = generate_world(&mut rng, 10);
            for npc in world.npcs.values() {
                if npc.disposition == crate::state::Disposition::Hostile {
                    assert!(
                        npc.combat_stats.is_some(),
                        "seed {}: hostile NPC '{}' (id={}) missing combat_stats",
                        seed, npc.name, npc.id
                    );
                }
            }
        }
    }

    #[test]
    fn test_merchant_never_promoted_to_hostile() {
        // The at-least-one-hostile guarantee must never promote a Merchant NPC.
        // A Merchant becoming hostile violates the game's NPC role contract:
        // Merchants are always peaceful and available for trade.
        for seed in 0..2000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let world = generate_world(&mut rng, 10);
            for npc in world.npcs.values() {
                if npc.role == crate::state::NpcRole::Merchant {
                    assert_ne!(
                        npc.disposition,
                        crate::state::Disposition::Hostile,
                        "seed {}: Merchant NPC '{}' (id={}) was promoted to Hostile. \
                         Merchants must never be hostile.",
                        seed, npc.name, npc.id
                    );
                }
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

    #[test]
    fn test_world_generation_deterministic_npc_locations() {
        // The same seed must always produce the same NPC locations.
        // Previously, HashMap iteration order in generate_npcs caused
        // location_ids to be in arbitrary order, making NPC placement
        // non-deterministic across process invocations.
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let w1 = generate_world(&mut rng1, 15);
        let w2 = generate_world(&mut rng2, 15);

        for id in w1.npcs.keys() {
            let npc1 = &w1.npcs[id];
            let npc2 = &w2.npcs[id];
            assert_eq!(npc1.location, npc2.location,
                "NPC {} ('{}') placed at different locations across runs: {} vs {}",
                id, npc1.name, npc1.location, npc2.location);
            assert_eq!(npc1.name, npc2.name);
            assert_eq!(
                npc1.combat_stats.as_ref().map(|s| s.max_hp),
                npc2.combat_stats.as_ref().map(|s| s.max_hp),
                "NPC {} ('{}') has different combat stats", id, npc1.name
            );
        }

        // Items should also be deterministic
        for id in w1.items.keys() {
            let item1 = &w1.items[id];
            let item2 = &w2.items[id];
            assert_eq!(item1.location, item2.location,
                "Item {} ('{}') placed at different locations: {:?} vs {:?}",
                id, item1.name, item1.location, item2.location);
        }
    }
}
