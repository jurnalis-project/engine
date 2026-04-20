// jurnalis-engine/src/world/location.rs
use crate::state::{LightLevel, Location, LocationType, RoomFeature};
use crate::types::{Direction, LocationId};
use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashMap;

const ROOM_NAMES: &[&str] = &[
    "Entrance Hall",
    "Narrow Passage",
    "Guard Room",
    "Storage Chamber",
    "Great Hall",
    "Collapsed Tunnel",
    "Damp Cellar",
    "Ancient Library",
    "Shrine",
    "Armory",
    "Prison Cell",
    "Throne Room",
    "Crypt",
    "Forge",
    "Well Chamber",
    "Hidden Alcove",
    "Moss-Covered Grotto",
    "Pillared Hall",
    "Winding Stairs",
    "Dead End",
];

const ROOM_DESCRIPTIONS: &[&str] = &[
    "The air is stale and cold.",
    "Water drips from the ceiling in a steady rhythm.",
    "Dust motes float in a shaft of dim light.",
    "The walls are carved with faded runes.",
    "Cobwebs stretch across every corner.",
    "The stone floor is cracked and uneven.",
    "A faint breeze whispers through the darkness.",
    "Old torches line the walls, long since extinguished.",
    "The room smells of damp earth and decay.",
    "Shadows dance at the edges of your vision.",
];

const ROOM_FEATURES: &[(&str, &str)] = &[
    (
        "altar",
        "Its worn surface is etched with old prayer marks and candle wax.",
    ),
    (
        "statue",
        "A weathered stone figure watches the room with a cracked, patient face.",
    ),
    (
        "door",
        "Age-darkened wood and iron bands suggest it has resisted many hands.",
    ),
    (
        "runes",
        "The faded runes look ceremonial, their grooves still sharp beneath the dust.",
    ),
    (
        "torch sconce",
        "Cold iron brackets hold the remains of long-burned torches.",
    ),
    (
        "bookshelf",
        "Warped shelves sag under mildew-stained books and loose parchment.",
    ),
    (
        "well",
        "A ring of damp stone surrounds a shaft that drops into darkness.",
    ),
    (
        "forge",
        "Ash and old slag cling to the stone, hinting at fires that once roared here.",
    ),
    (
        "chains",
        "Rust-bitten chains hang from the wall, some snapped, some still taut.",
    ),
    (
        "mural",
        "A peeling mural depicts forgotten figures in triumph and ruin.",
    ),
];

fn generate_room_features(rng: &mut impl Rng) -> Vec<RoomFeature> {
    let feature_count = rng.gen_range(1..=2);
    let mut pool: Vec<_> = ROOM_FEATURES.to_vec();
    pool.shuffle(rng);
    pool.into_iter()
        .take(feature_count)
        .map(|(name, description)| RoomFeature {
            name: name.to_string(),
            description: description.to_string(),
        })
        .collect()
}

const LOCATION_TYPES: &[LocationType] = &[
    LocationType::Room,
    LocationType::Corridor,
    LocationType::Cave,
    LocationType::Ruins,
];

fn opposite(dir: Direction) -> Direction {
    match dir {
        Direction::North => Direction::South,
        Direction::South => Direction::North,
        Direction::East => Direction::West,
        Direction::West => Direction::East,
        Direction::Up => Direction::Down,
        Direction::Down => Direction::Up,
    }
}

pub fn generate_locations(rng: &mut impl Rng, count: usize) -> HashMap<LocationId, Location> {
    let count = count.max(2).min(30);
    let mut locations: HashMap<LocationId, Location> = HashMap::new();

    // Pre-shuffle room names for uniqueness when count <= pool size
    let mut room_name_pool: Vec<&str> = ROOM_NAMES.to_vec();
    room_name_pool.shuffle(rng);

    // Create locations
    for i in 0..count {
        let id = i as LocationId;
        let name = if i < room_name_pool.len() {
            room_name_pool[i]
        } else {
            ROOM_NAMES[rng.gen_range(0..ROOM_NAMES.len())]
        };
        let desc_idx = rng.gen_range(0..ROOM_DESCRIPTIONS.len());
        let type_idx = rng.gen_range(0..LOCATION_TYPES.len());
        let light = match rng.gen_range(0..3) {
            0 => LightLevel::Bright,
            1 => LightLevel::Dim,
            _ => LightLevel::Dark,
        };

        locations.insert(
            id,
            Location {
                id,
                name: format!("{}", name),
                description: ROOM_DESCRIPTIONS[desc_idx].to_string(),
                location_type: LOCATION_TYPES[type_idx],
                exits: HashMap::new(),
                npcs: Vec::new(),
                items: Vec::new(),
                triggers: Vec::new(),
                light_level: light,
                room_features: generate_room_features(rng),
            },
        );
    }

    // Connect locations in a chain to ensure connectivity
    let directions = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
    ];
    for i in 0..(count - 1) {
        let from = i as LocationId;
        let to = (i + 1) as LocationId;

        // Find an available direction to guarantee connectivity
        let start = rng.gen_range(0..directions.len());
        let mut connected = false;
        for offset in 0..directions.len() {
            let dir = directions[(start + offset) % directions.len()];
            if !locations[&from].exits.contains_key(&dir)
                && !locations[&to].exits.contains_key(&opposite(dir))
            {
                locations.get_mut(&from).unwrap().exits.insert(dir, to);
                locations
                    .get_mut(&to)
                    .unwrap()
                    .exits
                    .insert(opposite(dir), from);
                connected = true;
                break;
            }
        }
        // If all 4 directions are taken (shouldn't happen in practice with small counts),
        // force-overwrite one to maintain connectivity
        if !connected {
            let dir = directions[start % directions.len()];
            locations.get_mut(&from).unwrap().exits.insert(dir, to);
            locations
                .get_mut(&to)
                .unwrap()
                .exits
                .insert(opposite(dir), from);
        }
    }

    // Add some extra connections for variety (branching)
    let extra = count / 3;
    for _ in 0..extra {
        let from = rng.gen_range(0..count) as LocationId;
        let to = rng.gen_range(0..count) as LocationId;
        if from == to {
            continue;
        }
        let dir = directions[rng.gen_range(0..directions.len())];
        if !locations[&from].exits.contains_key(&dir)
            && !locations[&to].exits.contains_key(&opposite(dir))
        {
            locations.get_mut(&from).unwrap().exits.insert(dir, to);
            locations
                .get_mut(&to)
                .unwrap()
                .exits
                .insert(opposite(dir), from);
        }
    }

    locations
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn test_generates_correct_count() {
        let mut rng = StdRng::seed_from_u64(42);
        let locs = generate_locations(&mut rng, 15);
        assert_eq!(locs.len(), 15);
    }

    #[test]
    fn test_all_locations_connected() {
        let mut rng = StdRng::seed_from_u64(42);
        let locs = generate_locations(&mut rng, 15);

        // BFS from location 0 should reach all locations
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(0u32);
        visited.insert(0u32);

        while let Some(loc_id) = queue.pop_front() {
            for &next_id in locs[&loc_id].exits.values() {
                if visited.insert(next_id) {
                    queue.push_back(next_id);
                }
            }
        }

        assert_eq!(visited.len(), locs.len(), "Not all locations are reachable");
    }

    #[test]
    fn test_exits_are_bidirectional() {
        let mut rng = StdRng::seed_from_u64(42);
        let locs = generate_locations(&mut rng, 10);

        for (id, loc) in &locs {
            for (dir, &target) in &loc.exits {
                let target_loc = &locs[&target];
                assert!(
                    target_loc.exits.get(&opposite(*dir)) == Some(id),
                    "Exit from {} {:?} to {} is not bidirectional",
                    id,
                    dir,
                    target
                );
            }
        }
    }

    #[test]
    fn test_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let locs1 = generate_locations(&mut rng1, 10);
        let locs2 = generate_locations(&mut rng2, 10);

        for id in 0..10u32 {
            assert_eq!(locs1[&id].name, locs2[&id].name);
            assert_eq!(locs1[&id].exits.len(), locs2[&id].exits.len());
        }
    }

    #[test]
    fn test_generate_locations_uses_unique_names_when_count_within_pool() {
        let mut rng = StdRng::seed_from_u64(42);
        let locs = generate_locations(&mut rng, 12);

        let mut names: Vec<String> = locs.values().map(|l| l.name.clone()).collect();
        names.sort();
        names.dedup();

        assert_eq!(
            names.len(),
            locs.len(),
            "Location names should be unique at this size"
        );
    }

    #[test]
    fn test_locations_generate_room_features() {
        let mut rng = StdRng::seed_from_u64(42);
        let locs = generate_locations(&mut rng, 5);

        assert!(locs.values().all(|loc| !loc.room_features.is_empty()));
    }
}
