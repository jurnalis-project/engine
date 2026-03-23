// jurnalis-engine/src/world/location.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{LocationId, Direction};
use crate::state::{Location, LocationType, LightLevel};

const ROOM_NAMES: &[&str] = &[
    "Entrance Hall", "Narrow Passage", "Guard Room", "Storage Chamber",
    "Great Hall", "Collapsed Tunnel", "Damp Cellar", "Ancient Library",
    "Shrine", "Armory", "Prison Cell", "Throne Room",
    "Crypt", "Forge", "Well Chamber", "Hidden Alcove",
    "Moss-Covered Grotto", "Pillared Hall", "Winding Stairs", "Dead End",
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

    // Create locations
    for i in 0..count {
        let id = i as LocationId;
        let name_idx = rng.gen_range(0..ROOM_NAMES.len());
        let desc_idx = rng.gen_range(0..ROOM_DESCRIPTIONS.len());
        let type_idx = rng.gen_range(0..LOCATION_TYPES.len());
        let light = match rng.gen_range(0..3) {
            0 => LightLevel::Bright,
            1 => LightLevel::Dim,
            _ => LightLevel::Dark,
        };

        locations.insert(id, Location {
            id,
            name: format!("{}", ROOM_NAMES[name_idx]),
            description: ROOM_DESCRIPTIONS[desc_idx].to_string(),
            location_type: LOCATION_TYPES[type_idx],
            exits: HashMap::new(),
            npcs: Vec::new(),
            items: Vec::new(),
            triggers: Vec::new(),
            light_level: light,
        });
    }

    // Connect locations in a chain to ensure connectivity
    let directions = [Direction::North, Direction::East, Direction::South, Direction::West];
    for i in 0..(count - 1) {
        let from = i as LocationId;
        let to = (i + 1) as LocationId;

        // Find an available direction to guarantee connectivity
        let start = rng.gen_range(0..directions.len());
        let mut connected = false;
        for offset in 0..directions.len() {
            let dir = directions[(start + offset) % directions.len()];
            if !locations[&from].exits.contains_key(&dir) && !locations[&to].exits.contains_key(&opposite(dir)) {
                locations.get_mut(&from).unwrap().exits.insert(dir, to);
                locations.get_mut(&to).unwrap().exits.insert(opposite(dir), from);
                connected = true;
                break;
            }
        }
        // If all 4 directions are taken (shouldn't happen in practice with small counts),
        // force-overwrite one to maintain connectivity
        if !connected {
            let dir = directions[start % directions.len()];
            locations.get_mut(&from).unwrap().exits.insert(dir, to);
            locations.get_mut(&to).unwrap().exits.insert(opposite(dir), from);
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
        if !locations[&from].exits.contains_key(&dir) && !locations[&to].exits.contains_key(&opposite(dir)) {
            locations.get_mut(&from).unwrap().exits.insert(dir, to);
            locations.get_mut(&to).unwrap().exits.insert(opposite(dir), from);
        }
    }

    locations
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

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
                    id, dir, target
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
}
