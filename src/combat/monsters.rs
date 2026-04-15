// jurnalis-engine/src/combat/monsters.rs
// SRD monster const table for combat encounters.

use std::collections::HashMap;
use rand::Rng;
use crate::types::Ability;
use crate::state::{CombatStats, NpcAttack, DamageType};

/// Static monster definition for the const table.
pub struct MonsterDef {
    pub name: &'static str,
    pub max_hp: i32,
    pub ac: i32,
    pub speed: i32,
    pub str_: i32,
    pub dex: i32,
    pub con: i32,
    pub int: i32,
    pub wis: i32,
    pub cha: i32,
    pub proficiency_bonus: i32,
    pub attacks: &'static [MonsterAttackDef],
    /// SRD challenge rating. Drives XP awarded on defeat (see `leveling::xp_for_cr`).
    /// Fractional values match SRD: 0, 1/8 = 0.125, 1/4 = 0.25, 1/2 = 0.5, etc.
    pub cr: f32,
}

pub struct MonsterAttackDef {
    pub name: &'static str,
    pub hit_bonus: i32,
    pub damage_dice: u32,
    pub damage_die: u32,
    pub damage_bonus: i32,
    pub damage_type: DamageType,
    pub reach: u32,
    pub range_normal: u32,
    pub range_long: u32,
}

// SRD monster table (~12 monsters)
pub const SRD_MONSTERS: &[MonsterDef] = &[
    MonsterDef {
        name: "Rat", max_hp: 1, ac: 10, speed: 20,
        str_: 2, dex: 11, con: 9, int: 2, wis: 10, cha: 4,
        proficiency_bonus: 2,
        cr: 0.0,
        attacks: &[MonsterAttackDef {
            name: "Bite", hit_bonus: 0, damage_dice: 1, damage_die: 1, damage_bonus: 0,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
        }],
    },
    MonsterDef {
        name: "Kobold", max_hp: 5, ac: 12, speed: 30,
        str_: 7, dex: 15, con: 9, int: 8, wis: 7, cha: 8,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[MonsterAttackDef {
            name: "Dagger", hit_bonus: 4, damage_dice: 1, damage_die: 4, damage_bonus: 2,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 20, range_long: 60,
        }],
    },
    MonsterDef {
        name: "Goblin", max_hp: 7, ac: 15, speed: 30,
        str_: 8, dex: 14, con: 10, int: 10, wis: 8, cha: 8,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[
            MonsterAttackDef {
                name: "Scimitar", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Shortbow", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
    },
    MonsterDef {
        name: "Skeleton", max_hp: 13, ac: 13, speed: 30,
        str_: 10, dex: 14, con: 15, int: 6, wis: 8, cha: 5,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[
            MonsterAttackDef {
                name: "Shortsword", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Shortbow", hit_bonus: 4, damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
    },
    MonsterDef {
        name: "Zombie", max_hp: 22, ac: 8, speed: 20,
        str_: 13, dex: 6, con: 16, int: 3, wis: 6, cha: 5,
        proficiency_bonus: 2,
        cr: 0.25,
        attacks: &[MonsterAttackDef {
            name: "Slam", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
            damage_type: DamageType::Bludgeoning, reach: 5, range_normal: 0, range_long: 0,
        }],
    },
    MonsterDef {
        name: "Guard", max_hp: 11, ac: 16, speed: 30,
        str_: 13, dex: 12, con: 12, int: 10, wis: 11, cha: 10,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[MonsterAttackDef {
            name: "Spear", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
            damage_type: DamageType::Piercing, reach: 5, range_normal: 20, range_long: 60,
        }],
    },
    MonsterDef {
        name: "Bandit", max_hp: 11, ac: 12, speed: 30,
        str_: 11, dex: 12, con: 12, int: 10, wis: 10, cha: 10,
        proficiency_bonus: 2,
        cr: 0.125,
        attacks: &[
            MonsterAttackDef {
                name: "Scimitar", hit_bonus: 3, damage_dice: 1, damage_die: 6, damage_bonus: 1,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Light Crossbow", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 80, range_long: 320,
            },
        ],
    },
    MonsterDef {
        name: "Orc", max_hp: 15, ac: 13, speed: 30,
        str_: 16, dex: 12, con: 16, int: 7, wis: 11, cha: 10,
        proficiency_bonus: 2,
        cr: 0.5,
        attacks: &[
            MonsterAttackDef {
                name: "Greataxe", hit_bonus: 5, damage_dice: 1, damage_die: 12, damage_bonus: 3,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 5, damage_dice: 1, damage_die: 6, damage_bonus: 3,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
    },
    MonsterDef {
        name: "Hobgoblin", max_hp: 11, ac: 18, speed: 30,
        str_: 13, dex: 12, con: 12, int: 10, wis: 10, cha: 9,
        proficiency_bonus: 2,
        cr: 0.5,
        attacks: &[
            MonsterAttackDef {
                name: "Longsword", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Longbow", hit_bonus: 3, damage_dice: 1, damage_die: 8, damage_bonus: 1,
                damage_type: DamageType::Piercing, reach: 0, range_normal: 150, range_long: 600,
            },
        ],
    },
    MonsterDef {
        name: "Bugbear", max_hp: 27, ac: 16, speed: 30,
        str_: 15, dex: 14, con: 13, int: 8, wis: 11, cha: 9,
        proficiency_bonus: 2,
        cr: 1.0,
        attacks: &[
            MonsterAttackDef {
                name: "Morningstar", hit_bonus: 4, damage_dice: 2, damage_die: 8, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 4, damage_dice: 2, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
    },
    MonsterDef {
        name: "Ghoul", max_hp: 22, ac: 12, speed: 30,
        str_: 13, dex: 15, con: 10, int: 7, wis: 10, cha: 6,
        proficiency_bonus: 2,
        cr: 1.0,
        attacks: &[
            MonsterAttackDef {
                name: "Claws", hit_bonus: 4, damage_dice: 2, damage_die: 4, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Bite", hit_bonus: 2, damage_dice: 2, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 0, range_long: 0,
            },
        ],
    },
    MonsterDef {
        name: "Ogre", max_hp: 59, ac: 11, speed: 40,
        str_: 19, dex: 8, con: 16, int: 5, wis: 7, cha: 7,
        proficiency_bonus: 2,
        cr: 2.0,
        attacks: &[
            MonsterAttackDef {
                name: "Greatclub", hit_bonus: 6, damage_dice: 2, damage_die: 8, damage_bonus: 4,
                damage_type: DamageType::Bludgeoning, reach: 5, range_normal: 0, range_long: 0,
            },
            MonsterAttackDef {
                name: "Javelin", hit_bonus: 6, damage_dice: 2, damage_die: 6, damage_bonus: 4,
                damage_type: DamageType::Piercing, reach: 5, range_normal: 30, range_long: 120,
            },
        ],
    },
];

/// Look up a monster definition by name (case-insensitive).
pub fn find_monster(name: &str) -> Option<&'static MonsterDef> {
    let lower = name.to_lowercase();
    SRD_MONSTERS.iter().find(|m| m.name.to_lowercase() == lower)
}

/// Convert a MonsterDef into a CombatStats instance.
pub fn monster_to_combat_stats(def: &MonsterDef) -> CombatStats {
    let mut ability_scores = HashMap::new();
    ability_scores.insert(Ability::Strength, def.str_);
    ability_scores.insert(Ability::Dexterity, def.dex);
    ability_scores.insert(Ability::Constitution, def.con);
    ability_scores.insert(Ability::Intelligence, def.int);
    ability_scores.insert(Ability::Wisdom, def.wis);
    ability_scores.insert(Ability::Charisma, def.cha);

    let attacks = def.attacks.iter().map(|a| NpcAttack {
        name: a.name.to_string(),
        hit_bonus: a.hit_bonus,
        damage_dice: a.damage_dice,
        damage_die: a.damage_die,
        damage_bonus: a.damage_bonus,
        damage_type: a.damage_type,
        reach: a.reach,
        range_normal: a.range_normal,
        range_long: a.range_long,
    }).collect();

    CombatStats {
        max_hp: def.max_hp,
        current_hp: def.max_hp,
        ac: def.ac,
        speed: def.speed,
        ability_scores,
        attacks,
        proficiency_bonus: def.proficiency_bonus,
    }
}

/// HP target window for a given depth tier. Depth is a location index proxy
/// (0 = entrance, increasing with distance from spawn). See
/// `docs/specs/world-generation.md` for the authoritative definition.
fn hp_window_for_depth(depth: usize) -> (i32, i32) {
    match depth {
        0..=3 => (5, 12),
        4..=8 => (10, 18),
        _ => (15, 25),
    }
}

/// Pick an `SRD_MONSTERS` entry whose `max_hp` falls within the depth's
/// target window. If no monster matches the window (defensive fallback),
/// return the monster whose `max_hp` is closest to the window's midpoint,
/// with ties broken by table order.
///
/// This biases early rooms (low depth) toward weaker foes, scaling up with
/// distance from the player's spawn. Selection is deterministic for a given
/// RNG state, preserving world-generation reproducibility.
pub fn select_monster_for_depth(rng: &mut impl Rng, depth: usize) -> &'static MonsterDef {
    let (lo, hi) = hp_window_for_depth(depth);

    let matching: Vec<&'static MonsterDef> = SRD_MONSTERS
        .iter()
        .filter(|m| m.max_hp >= lo && m.max_hp <= hi)
        .collect();

    if !matching.is_empty() {
        let idx = rng.gen_range(0..matching.len());
        return matching[idx];
    }

    // Defensive fallback: pick the monster closest to the window midpoint.
    // Ties are broken by table order (first occurrence wins).
    let mid = (lo + hi) / 2;
    SRD_MONSTERS
        .iter()
        .min_by_key(|m| (m.max_hp - mid).abs())
        .expect("SRD_MONSTERS must not be empty")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_srd_monsters_count() {
        assert_eq!(SRD_MONSTERS.len(), 12);
    }

    #[test]
    fn test_select_monster_for_depth_tier_0_hp_range() {
        // Depth 0-3 should bias toward HP 5-12.
        for depth in 0..=3 {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 5 && def.max_hp <= 12,
                    "depth {} seed {}: picked {} with HP {}, expected 5-12",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_tier_1_hp_range() {
        // Depth 4-8 should bias toward HP 10-18.
        for depth in 4..=8 {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 10 && def.max_hp <= 18,
                    "depth {} seed {}: picked {} with HP {}, expected 10-18",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_tier_2_hp_range() {
        // Depth 9+ should bias toward HP 15-25.
        for depth in [9usize, 12, 20, 100] {
            for seed in 0..32u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let def = select_monster_for_depth(&mut rng, depth);
                assert!(
                    def.max_hp >= 15 && def.max_hp <= 25,
                    "depth {} seed {}: picked {} with HP {}, expected 15-25",
                    depth, seed, def.name, def.max_hp
                );
            }
        }
    }

    #[test]
    fn test_select_monster_for_depth_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(7);
        let mut rng2 = StdRng::seed_from_u64(7);
        let a = select_monster_for_depth(&mut rng1, 2);
        let b = select_monster_for_depth(&mut rng2, 2);
        assert_eq!(a.name, b.name);
        assert_eq!(a.max_hp, b.max_hp);
    }

    #[test]
    fn test_find_monster_by_name() {
        assert!(find_monster("Goblin").is_some());
        assert!(find_monster("goblin").is_some());
        assert!(find_monster("nonexistent").is_none());
    }

    #[test]
    fn test_goblin_stats() {
        let goblin = find_monster("Goblin").unwrap();
        assert_eq!(goblin.max_hp, 7);
        assert_eq!(goblin.ac, 15);
        assert_eq!(goblin.speed, 30);
        assert_eq!(goblin.attacks.len(), 2);
    }

    #[test]
    fn test_ogre_stats() {
        let ogre = find_monster("Ogre").unwrap();
        assert_eq!(ogre.max_hp, 59);
        assert_eq!(ogre.ac, 11);
        assert_eq!(ogre.str_, 19);
    }

    #[test]
    fn test_monster_to_combat_stats() {
        let goblin = find_monster("Goblin").unwrap();
        let stats = monster_to_combat_stats(goblin);
        assert_eq!(stats.max_hp, 7);
        assert_eq!(stats.current_hp, 7);
        assert_eq!(stats.ac, 15);
        assert_eq!(stats.speed, 30);
        assert_eq!(stats.attacks.len(), 2);
        assert_eq!(stats.attacks[0].name, "Scimitar");
        assert_eq!(*stats.ability_scores.get(&Ability::Dexterity).unwrap(), 14);
    }

    #[test]
    fn test_all_monsters_have_attacks() {
        for monster in SRD_MONSTERS {
            assert!(!monster.attacks.is_empty(), "{} has no attacks", monster.name);
        }
    }

    #[test]
    fn test_all_monsters_positive_hp() {
        for monster in SRD_MONSTERS {
            assert!(monster.max_hp > 0, "{} has non-positive HP", monster.name);
        }
    }

    #[test]
    fn test_all_monsters_have_finite_nonneg_cr() {
        for monster in SRD_MONSTERS {
            assert!(monster.cr.is_finite(), "{} has non-finite CR", monster.name);
            assert!(monster.cr >= 0.0, "{} has negative CR", monster.name);
        }
    }

    #[test]
    fn test_canonical_monster_crs() {
        // Spot-check: all entries from the leveling spec table.
        assert_eq!(find_monster("Rat").unwrap().cr, 0.0);
        assert_eq!(find_monster("Kobold").unwrap().cr, 0.125);
        assert_eq!(find_monster("Goblin").unwrap().cr, 0.25);
        assert_eq!(find_monster("Skeleton").unwrap().cr, 0.25);
        assert_eq!(find_monster("Zombie").unwrap().cr, 0.25);
        assert_eq!(find_monster("Guard").unwrap().cr, 0.125);
        assert_eq!(find_monster("Bandit").unwrap().cr, 0.125);
        assert_eq!(find_monster("Orc").unwrap().cr, 0.5);
        assert_eq!(find_monster("Hobgoblin").unwrap().cr, 0.5);
        assert_eq!(find_monster("Bugbear").unwrap().cr, 1.0);
        assert_eq!(find_monster("Ghoul").unwrap().cr, 1.0);
        assert_eq!(find_monster("Ogre").unwrap().cr, 2.0);
    }

}
