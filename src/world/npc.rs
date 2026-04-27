// jurnalis-engine/src/world/npc.rs
use crate::state::{Disposition, Location, Npc, NpcRole};
use crate::types::{LocationId, NpcId};
use rand::Rng;
use std::collections::HashMap;

impl Npc {
    /// Return the NPC name with a disposition/role tag appended.
    ///
    /// Tag derivation:
    /// - `NpcRole::Merchant` -> `[merchant]` (regardless of disposition)
    /// - `Disposition::Hostile` -> `[hostile]`
    /// - `Disposition::Neutral` or `Disposition::Friendly` -> `[neutral]`
    pub fn display_name(&self) -> String {
        let tag = if self.role == NpcRole::Merchant {
            "merchant"
        } else {
            match self.disposition {
                Disposition::Hostile => "hostile",
                Disposition::Neutral | Disposition::Friendly => "neutral",
            }
        };
        format!("{} [{}]", self.name, tag)
    }

    /// Return a multi-line inspection description for the NPC.
    /// Lines:
    ///   1. NPC full name with disposition tag (e.g. "Orin the Quiet [hostile]")
    ///   2. Role description (e.g. "A wandering merchant.")
    ///   3. Disposition sentence (e.g. "They seem friendly.")
    ///   4+ (optional) Visible special traits, one per line, indented with two spaces.
    pub fn inspect(&self) -> Vec<String> {
        let mut lines = Vec::new();

        lines.push(self.display_name());

        let role_line = match self.role {
            NpcRole::Merchant => "A wandering merchant.",
            NpcRole::Guard => "A watchful guard.",
            NpcRole::Hermit => "A solitary hermit.",
            NpcRole::Adventurer => "A seasoned adventurer.",
        };
        lines.push(role_line.to_string());

        let disposition_line = match self.disposition {
            Disposition::Friendly => "They seem friendly.",
            Disposition::Neutral => "They regard you neutrally.",
            Disposition::Hostile => "They eye you with hostility.",
        };
        lines.push(disposition_line.to_string());

        if let Some(stats) = &self.combat_stats {
            for (trait_name, trait_desc) in &stats.special_traits {
                lines.push(format!("  {}: {}", trait_name, trait_desc));
            }
        }

        lines
    }
}

const FIRST_NAMES: &[&str] = &[
    "Aldric", "Brenna", "Corwin", "Dara", "Eldon", "Fiona", "Gareth", "Helena", "Ivar", "Jasmine",
    "Kael", "Lyra", "Magnus", "Nessa", "Orin", "Petra", "Quinn", "Rowan", "Seren", "Theron",
];

const TITLES: &[&str] = &[
    "the Wanderer",
    "the Bold",
    "the Quiet",
    "the Old",
    "the Scarred",
    "the Wise",
    "the Lost",
    "the Keeper",
];

const DIALOGUE_TAGS_BY_ROLE: &[(NpcRole, &[&str])] = &[
    (NpcRole::Merchant, &["trade", "goods", "prices", "wares"]),
    (NpcRole::Guard, &["warning", "danger", "patrol", "orders"]),
    (NpcRole::Hermit, &["riddle", "lore", "cryptic", "ancient"]),
    (
        NpcRole::Adventurer,
        &["quest", "treasure", "adventure", "rumor"],
    ),
];

pub fn generate_npcs(
    rng: &mut impl Rng,
    locations: &HashMap<LocationId, &Location>,
    npc_count: usize,
) -> HashMap<NpcId, Npc> {
    let mut npcs = HashMap::new();
    let mut location_ids: Vec<LocationId> = locations.keys().copied().collect();
    location_ids.sort();
    if location_ids.is_empty() {
        return npcs;
    }

    let roles = [
        NpcRole::Merchant,
        NpcRole::Guard,
        NpcRole::Hermit,
        NpcRole::Adventurer,
    ];
    let dispositions = [
        Disposition::Friendly,
        Disposition::Neutral,
        Disposition::Hostile,
    ];

    for i in 0..npc_count {
        let id = i as NpcId;
        let name_idx = rng.gen_range(0..FIRST_NAMES.len());
        let title_idx = rng.gen_range(0..TITLES.len());
        let role = roles[rng.gen_range(0..roles.len())];
        // Merchants must never be hostile — they are always peaceful and
        // available for trade. Draw disposition from the full pool to keep
        // RNG parity, then clamp Hostile → Neutral for Merchants.
        let mut disposition = dispositions[rng.gen_range(0..dispositions.len())];
        if role == NpcRole::Merchant && disposition == Disposition::Hostile {
            disposition = Disposition::Neutral;
        }
        let location = location_ids[rng.gen_range(0..location_ids.len())];

        let dialogue_tags = DIALOGUE_TAGS_BY_ROLE
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, tags)| tags.iter().map(|s| s.to_string()).collect())
            .unwrap_or_default();

        npcs.insert(
            id,
            Npc {
                id,
                name: format!("{} {}", FIRST_NAMES[name_idx], TITLES[title_idx]),
                role,
                disposition,
                dialogue_tags,
                location,
                combat_stats: None,
                conditions: Vec::new(),
            },
        );
    }

    npcs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{LightLevel, LocationType};
    use rand::rngs::StdRng;
    use rand::SeedableRng;

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
            room_features: Vec::new(),
        }
    }

    fn make_npc(role: NpcRole, disposition: Disposition) -> Npc {
        Npc {
            id: 0,
            name: "Orin the Quiet".to_string(),
            role,
            disposition,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: None,
            conditions: vec![],
        }
    }

    #[test]
    fn test_display_name_hostile_npc_shows_hostile_tag() {
        let npc = make_npc(NpcRole::Guard, Disposition::Hostile);
        assert_eq!(npc.display_name(), "Orin the Quiet [hostile]");
    }

    #[test]
    fn test_display_name_neutral_npc_shows_neutral_tag() {
        let npc = make_npc(NpcRole::Guard, Disposition::Neutral);
        assert_eq!(npc.display_name(), "Orin the Quiet [neutral]");
    }

    #[test]
    fn test_display_name_friendly_npc_shows_neutral_tag() {
        let npc = make_npc(NpcRole::Hermit, Disposition::Friendly);
        assert_eq!(npc.display_name(), "Orin the Quiet [neutral]");
    }

    #[test]
    fn test_display_name_merchant_shows_merchant_tag() {
        let npc = make_npc(NpcRole::Merchant, Disposition::Friendly);
        assert_eq!(npc.display_name(), "Orin the Quiet [merchant]");
    }

    #[test]
    fn test_display_name_merchant_neutral_shows_merchant_tag() {
        let npc = make_npc(NpcRole::Merchant, Disposition::Neutral);
        assert_eq!(npc.display_name(), "Orin the Quiet [merchant]");
    }

    #[test]
    fn test_inspect_header_includes_disposition_tag() {
        let hostile = make_npc(NpcRole::Guard, Disposition::Hostile);
        let lines = hostile.inspect();
        assert_eq!(
            lines[0], "Orin the Quiet [hostile]",
            "Inspect header should include [hostile] tag. Got: {:?}",
            lines[0]
        );

        let merchant = make_npc(NpcRole::Merchant, Disposition::Friendly);
        let lines = merchant.inspect();
        assert_eq!(
            lines[0], "Orin the Quiet [merchant]",
            "Inspect header should include [merchant] tag. Got: {:?}",
            lines[0]
        );

        let neutral = make_npc(NpcRole::Hermit, Disposition::Neutral);
        let lines = neutral.inspect();
        assert_eq!(
            lines[0], "Orin the Quiet [neutral]",
            "Inspect header should include [neutral] tag. Got: {:?}",
            lines[0]
        );
    }

    #[test]
    fn test_inspect_returns_name_as_first_line() {
        let npc = make_npc(NpcRole::Hermit, Disposition::Neutral);
        let lines = npc.inspect();
        assert_eq!(lines[0], "Orin the Quiet [neutral]");
    }

    #[test]
    fn test_inspect_includes_role_description() {
        let merchant = make_npc(NpcRole::Merchant, Disposition::Friendly);
        let lines = merchant.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("merchant")),
            "Expected 'merchant' in: {:?}",
            lines
        );

        let guard = make_npc(NpcRole::Guard, Disposition::Neutral);
        let lines = guard.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("guard")),
            "Expected 'guard' in: {:?}",
            lines
        );

        let hermit = make_npc(NpcRole::Hermit, Disposition::Hostile);
        let lines = hermit.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("hermit")),
            "Expected 'hermit' in: {:?}",
            lines
        );

        let adventurer = make_npc(NpcRole::Adventurer, Disposition::Friendly);
        let lines = adventurer.inspect();
        assert!(
            lines
                .iter()
                .any(|l| l.to_lowercase().contains("adventurer")),
            "Expected 'adventurer' in: {:?}",
            lines
        );
    }

    #[test]
    fn test_inspect_includes_disposition() {
        let friendly = make_npc(NpcRole::Guard, Disposition::Friendly);
        let lines = friendly.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("friendly")),
            "Expected 'friendly' in: {:?}",
            lines
        );

        let hostile = make_npc(NpcRole::Guard, Disposition::Hostile);
        let lines = hostile.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("hostil")),
            "Expected 'hostile' in: {:?}",
            lines
        );

        let neutral = make_npc(NpcRole::Guard, Disposition::Neutral);
        let lines = neutral.inspect();
        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("neutral")),
            "Expected 'neutral' in: {:?}",
            lines
        );
    }

    #[test]
    fn test_inspect_shows_special_traits() {
        use crate::state::CombatStats;
        let mut npc = make_npc(NpcRole::Adventurer, Disposition::Friendly);
        npc.combat_stats = Some(CombatStats {
            special_traits: vec![(
                "Pack Tactics".to_string(),
                "Advantage when ally is adjacent.".to_string(),
            )],
            ..CombatStats::default()
        });
        let lines = npc.inspect();
        assert!(
            lines.iter().any(|l| l.contains("Pack Tactics")),
            "Expected 'Pack Tactics' in: {:?}",
            lines
        );
        assert!(
            lines
                .iter()
                .any(|l| l.contains("Advantage when ally is adjacent.")),
            "Expected trait description in: {:?}",
            lines
        );
    }

    #[test]
    fn test_inspect_no_traits_when_no_combat_stats() {
        let npc = make_npc(NpcRole::Merchant, Disposition::Neutral);
        let lines = npc.inspect();
        // Should have exactly 3 lines: name, role, disposition
        assert_eq!(lines.len(), 3, "Expected 3 lines, got: {:?}", lines);
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
