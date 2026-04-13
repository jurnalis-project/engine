// jurnalis-engine/src/world/npc.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{NpcId, LocationId};
use crate::state::{Npc, NpcRole, Disposition, Location};

const FIRST_NAMES: &[&str] = &[
    "Aldric", "Brenna", "Corwin", "Dara", "Eldon", "Fiona",
    "Gareth", "Helena", "Ivar", "Jasmine", "Kael", "Lyra",
    "Magnus", "Nessa", "Orin", "Petra", "Quinn", "Rowan",
    "Seren", "Theron",
];

const TITLES: &[&str] = &[
    "the Wanderer", "the Bold", "the Quiet", "the Old",
    "the Scarred", "the Wise", "the Lost", "the Keeper",
];

const DIALOGUE_TAGS_BY_ROLE: &[(NpcRole, &[&str])] = &[
    (NpcRole::Merchant, &["trade", "goods", "prices", "wares"]),
    (NpcRole::Guard, &["warning", "danger", "patrol", "orders"]),
    (NpcRole::Hermit, &["riddle", "lore", "cryptic", "ancient"]),
    (NpcRole::Adventurer, &["quest", "treasure", "adventure", "rumor"]),
];

pub fn generate_npcs(
    rng: &mut impl Rng,
    locations: &HashMap<LocationId, &Location>,
    npc_count: usize,
) -> HashMap<NpcId, Npc> {
    let mut npcs = HashMap::new();
    let location_ids: Vec<LocationId> = locations.keys().copied().collect();
    if location_ids.is_empty() {
        return npcs;
    }

    let roles = [NpcRole::Merchant, NpcRole::Guard, NpcRole::Hermit, NpcRole::Adventurer];
    let dispositions = [Disposition::Friendly, Disposition::Neutral, Disposition::Hostile];

    for i in 0..npc_count {
        let id = i as NpcId;
        let name_idx = rng.gen_range(0..FIRST_NAMES.len());
        let title_idx = rng.gen_range(0..TITLES.len());
        let role = roles[rng.gen_range(0..roles.len())];
        let disposition = dispositions[rng.gen_range(0..dispositions.len())];
        let location = location_ids[rng.gen_range(0..location_ids.len())];

        let dialogue_tags = DIALOGUE_TAGS_BY_ROLE
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, tags)| tags.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        npcs.insert(id, Npc {
            id,
            name: format!("{} {}", FIRST_NAMES[name_idx], TITLES[title_idx]),
            role,
            disposition,
            dialogue_tags,
            location,
            combat_stats: None,
            conditions: Vec::new(),
        });
    }

    npcs
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use crate::state::{LocationType, LightLevel};

    fn dummy_location(id: LocationId) -> Location {
        Location {
            id,
            name: "Test".to_string(),
            description: "Test".to_string(),
            location_type: LocationType::Room,
            exits: HashMap::new(),
            npcs: Vec::new(),
            items: Vec::new(),
            triggers: Vec::new(),
            light_level: LightLevel::Bright,
        }
    }

    #[test]
    fn test_generates_correct_count() {
        let mut rng = StdRng::seed_from_u64(42);
        let loc = dummy_location(0);
        let locs: HashMap<LocationId, &Location> = [(0, &loc)].into_iter().collect();
        let npcs = generate_npcs(&mut rng, &locs, 5);
        assert_eq!(npcs.len(), 5);
    }

    #[test]
    fn test_npcs_have_names() {
        let mut rng = StdRng::seed_from_u64(42);
        let loc = dummy_location(0);
        let locs: HashMap<LocationId, &Location> = [(0, &loc)].into_iter().collect();
        let npcs = generate_npcs(&mut rng, &locs, 3);
        for npc in npcs.values() {
            assert!(!npc.name.is_empty());
        }
    }

    #[test]
    fn test_npcs_assigned_to_valid_locations() {
        let mut rng = StdRng::seed_from_u64(42);
        let loc0 = dummy_location(0);
        let loc1 = dummy_location(1);
        let locs: HashMap<LocationId, &Location> = [(0, &loc0), (1, &loc1)].into_iter().collect();
        let npcs = generate_npcs(&mut rng, &locs, 5);
        for npc in npcs.values() {
            assert!(locs.contains_key(&npc.location));
        }
    }
}
