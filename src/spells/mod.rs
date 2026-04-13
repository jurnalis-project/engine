// jurnalis-engine/src/spells/mod.rs
// Spell definitions, slot tracking, and casting resolution.
// Dependencies: types.rs, state/ only (no feature module imports).

use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::Ability;
use crate::rules::dice::{roll_d20, roll_dice};

/// Identifies a spell in the system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellDef {
    pub name: &'static str,
    pub level: u32,          // 0 = cantrip
    pub school: SpellSchool,
    pub casting: CastingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellSchool {
    Evocation,
    Transmutation,
    Enchantment,
    Abjuration,
}

/// How a spell resolves mechanically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastingMode {
    /// Ranged spell attack vs AC.
    SpellAttack,
    /// Auto-hit, no roll needed.
    AutoHit,
    /// Targets make a saving throw.
    SaveHalf { save_ability: Ability },
    /// Area effect by HP pool.
    HpPool,
    /// Self-buff.
    SelfBuff,
    /// Flavor only, no mechanical effect.
    Flavor,
}

/// All MVP spells.
pub const SPELLS: &[SpellDef] = &[
    SpellDef { name: "Fire Bolt", level: 0, school: SpellSchool::Evocation, casting: CastingMode::SpellAttack },
    SpellDef { name: "Prestidigitation", level: 0, school: SpellSchool::Transmutation, casting: CastingMode::Flavor },
    SpellDef { name: "Magic Missile", level: 1, school: SpellSchool::Evocation, casting: CastingMode::AutoHit },
    SpellDef { name: "Burning Hands", level: 1, school: SpellSchool::Evocation, casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity } },
    SpellDef { name: "Sleep", level: 1, school: SpellSchool::Enchantment, casting: CastingMode::HpPool },
    SpellDef { name: "Shield", level: 1, school: SpellSchool::Abjuration, casting: CastingMode::SelfBuff },
];

/// Look up a spell definition by name (case-insensitive).
pub fn find_spell(name: &str) -> Option<&'static SpellDef> {
    let lower = name.to_lowercase();
    SPELLS.iter().find(|s| s.name.to_lowercase() == lower)
}

/// Compute spell attack modifier: INT mod + proficiency bonus.
pub fn spell_attack_modifier(int_score: i32, proficiency_bonus: i32) -> i32 {
    Ability::modifier(int_score) + proficiency_bonus
}

/// Compute spell save DC: 8 + INT mod + proficiency bonus.
pub fn spell_save_dc(int_score: i32, proficiency_bonus: i32) -> i32 {
    8 + Ability::modifier(int_score) + proficiency_bonus
}

/// Result of a spell attack roll.
#[derive(Debug, Clone)]
pub struct SpellAttackResult {
    pub roll: i32,
    pub modifier: i32,
    pub total: i32,
    pub hit: bool,
    pub natural_20: bool,
    pub natural_1: bool,
}

/// Roll a spell attack against a target AC.
pub fn roll_spell_attack(
    rng: &mut impl Rng,
    int_score: i32,
    proficiency_bonus: i32,
    target_ac: i32,
) -> SpellAttackResult {
    let roll = roll_d20(rng);
    let modifier = spell_attack_modifier(int_score, proficiency_bonus);
    let total = roll + modifier;
    let natural_20 = roll == 20;
    let natural_1 = roll == 1;
    let hit = natural_20 || (!natural_1 && total >= target_ac);
    SpellAttackResult { roll, modifier, total, hit, natural_20, natural_1 }
}

/// Result of a spell save.
#[derive(Debug, Clone)]
pub struct SpellSaveResult {
    pub roll: i32,
    pub modifier: i32,
    pub total: i32,
    pub dc: i32,
    pub saved: bool,
}

/// Roll a saving throw against the caster's spell save DC.
pub fn roll_spell_save(
    rng: &mut impl Rng,
    save_ability_score: i32,
    save_proficiency_bonus: i32,
    is_proficient: bool,
    dc: i32,
) -> SpellSaveResult {
    let roll = roll_d20(rng);
    let modifier = Ability::modifier(save_ability_score) + if is_proficient { save_proficiency_bonus } else { 0 };
    let total = roll + modifier;
    SpellSaveResult { roll, modifier, total, dc, saved: total >= dc }
}

/// Outcome of a complete spell cast.
#[derive(Debug, Clone)]
pub enum CastOutcome {
    /// Not a spellcaster.
    NotACaster,
    /// Spell not known.
    UnknownSpell,
    /// No spell slots remaining.
    NoSlots,
    /// Spell not usable outside combat.
    NotInCombat,
    /// Fire Bolt: spell attack result + damage.
    FireBolt {
        attack: SpellAttackResult,
        damage: i32,
    },
    /// Prestidigitation flavor text.
    Prestidigitation,
    /// Magic Missile: auto-hit damage.
    MagicMissile {
        darts: Vec<i32>,
        total_damage: i32,
    },
    /// Burning Hands: per-target save results + damage.
    BurningHands {
        total_rolled: i32,
        half_damage: i32,
        dc: i32,
        results: Vec<BurningHandsTarget>,
    },
    /// Sleep: HP pool and affected targets.
    SleepResult {
        hp_pool: i32,
        affected: Vec<SleepTarget>,
    },
    /// Shield: AC bonus applied.
    ShieldCast {
        ac_bonus: i32,
    },
}

#[derive(Debug, Clone)]
pub struct BurningHandsTarget {
    pub name: String,
    pub save_result: SpellSaveResult,
    pub damage_taken: i32,
}

#[derive(Debug, Clone)]
pub struct SleepTarget {
    pub name: String,
    pub hp: i32,
}

/// Information about an enemy target needed for spell resolution.
/// This struct avoids importing combat or NPC types directly.
#[derive(Debug, Clone)]
pub struct SpellTarget {
    pub id: u32,
    pub name: String,
    pub ac: i32,
    pub current_hp: i32,
    pub ability_scores: std::collections::HashMap<Ability, i32>,
    pub proficiency_bonus: i32,
    pub save_proficiencies: Vec<Ability>,
    pub distance: u32,
}

/// Resolve a Fire Bolt cast against a single target.
pub fn resolve_fire_bolt(
    rng: &mut impl Rng,
    int_score: i32,
    proficiency_bonus: i32,
    target_ac: i32,
) -> CastOutcome {
    let attack = roll_spell_attack(rng, int_score, proficiency_bonus, target_ac);
    let damage = if attack.hit {
        let rolls = roll_dice(rng, 1, 10);
        let base: i32 = rolls.iter().sum();
        if attack.natural_20 { base * 2 } else { base }
    } else {
        0
    };
    CastOutcome::FireBolt { attack, damage }
}

/// Resolve Magic Missile (auto-hit, 3 darts of 1d4+1).
pub fn resolve_magic_missile(rng: &mut impl Rng) -> CastOutcome {
    let mut darts = Vec::new();
    for _ in 0..3 {
        let rolls = roll_dice(rng, 1, 4);
        darts.push(rolls.iter().sum::<i32>() + 1);
    }
    let total_damage = darts.iter().sum();
    CastOutcome::MagicMissile { darts, total_damage }
}

/// Resolve Burning Hands against all targets within 5 ft.
pub fn resolve_burning_hands(
    rng: &mut impl Rng,
    caster_int_score: i32,
    caster_proficiency_bonus: i32,
    targets: &[SpellTarget],
) -> CastOutcome {
    let dc = spell_save_dc(caster_int_score, caster_proficiency_bonus);
    let damage_rolls = roll_dice(rng, 3, 6);
    let total_rolled: i32 = damage_rolls.iter().sum();
    let half_damage = total_rolled / 2;

    let melee_targets: Vec<&SpellTarget> = targets.iter().filter(|t| t.distance <= 5).collect();

    let mut results = Vec::new();
    for target in melee_targets {
        let dex_score = target.ability_scores.get(&Ability::Dexterity).copied().unwrap_or(10);
        let is_prof = target.save_proficiencies.contains(&Ability::Dexterity);
        let save = roll_spell_save(rng, dex_score, target.proficiency_bonus, is_prof, dc);
        let damage_taken = if save.saved { half_damage } else { total_rolled };
        results.push(BurningHandsTarget {
            name: target.name.clone(),
            save_result: save,
            damage_taken,
        });
    }

    CastOutcome::BurningHands { total_rolled, half_damage, dc, results }
}

/// Resolve Sleep spell (5d8 HP pool, weakest first).
pub fn resolve_sleep(
    rng: &mut impl Rng,
    targets: &[SpellTarget],
) -> CastOutcome {
    let pool_rolls = roll_dice(rng, 5, 8);
    let mut hp_pool: i32 = pool_rolls.iter().sum();

    // Sort targets by current HP (weakest first)
    let mut sorted: Vec<&SpellTarget> = targets.iter().collect();
    sorted.sort_by_key(|t| t.current_hp);

    let mut affected = Vec::new();
    for target in sorted {
        if target.current_hp > 0 && target.current_hp <= hp_pool {
            hp_pool -= target.current_hp;
            affected.push(SleepTarget {
                name: target.name.clone(),
                hp: target.current_hp,
            });
        }
    }

    let total_pool: i32 = pool_rolls.iter().sum();
    CastOutcome::SleepResult { hp_pool: total_pool, affected }
}

/// Resolve Shield spell (+5 AC self-buff).
pub fn resolve_shield() -> CastOutcome {
    CastOutcome::ShieldCast { ac_bonus: 5 }
}

/// Format the player's known spells and remaining spell slots for display.
/// Returns lines suitable for the `spells` command output.
pub fn format_known_spells(
    known_spells: &[String],
    spell_slots_remaining: &[i32],
    spell_slots_max: &[i32],
) -> Vec<String> {
    if known_spells.is_empty() {
        return vec!["You don't know any spells.".to_string()];
    }

    let mut lines = Vec::new();
    lines.push("=== Known Spells ===".to_string());

    // Cantrips
    let cantrips: Vec<&String> = known_spells
        .iter()
        .filter(|name| {
            find_spell(name).map_or(false, |def| def.level == 0)
        })
        .collect();

    if !cantrips.is_empty() {
        lines.push(String::new());
        lines.push("Cantrips (at will):".to_string());
        for name in &cantrips {
            lines.push(format!("  - {}", name));
        }
    }

    // Level 1 spells
    let level1: Vec<&String> = known_spells
        .iter()
        .filter(|name| {
            find_spell(name).map_or(false, |def| def.level == 1)
        })
        .collect();

    if !level1.is_empty() {
        lines.push(String::new());
        lines.push("Level 1 Spells:".to_string());
        for name in &level1 {
            lines.push(format!("  - {}", name));
        }
    }

    // Spell slots
    if !spell_slots_max.is_empty() {
        lines.push(String::new());
        lines.push("Spell Slots:".to_string());
        for (i, (remaining, max)) in spell_slots_remaining.iter().zip(spell_slots_max.iter()).enumerate() {
            lines.push(format!("  Level {}: {}/{}", i + 1, remaining, max));
        }
    }

    lines
}

/// Check if a character can cast and consume a slot. Returns true if the slot was consumed.
/// For cantrips (level 0), always returns true without consuming slots.
pub fn consume_spell_slot(
    spell_level: u32,
    slots_remaining: &mut Vec<i32>,
) -> bool {
    if spell_level == 0 {
        return true; // cantrips don't consume slots
    }
    let idx = (spell_level - 1) as usize;
    if idx >= slots_remaining.len() || slots_remaining[idx] <= 0 {
        return false;
    }
    slots_remaining[idx] -= 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;

    fn test_target(name: &str, ac: i32, hp: i32, dex: i32, distance: u32) -> SpellTarget {
        let mut scores = HashMap::new();
        scores.insert(Ability::Dexterity, dex);
        SpellTarget {
            id: 0,
            name: name.to_string(),
            ac,
            current_hp: hp,
            ability_scores: scores,
            proficiency_bonus: 2,
            save_proficiencies: Vec::new(),
            distance,
        }
    }

    #[test]
    fn test_find_spell_case_insensitive() {
        assert!(find_spell("fire bolt").is_some());
        assert!(find_spell("Fire Bolt").is_some());
        assert!(find_spell("MAGIC MISSILE").is_some());
        assert!(find_spell("nonexistent").is_none());
    }

    #[test]
    fn test_spell_attack_modifier() {
        // INT 16 (+3) + prof 2 = +5
        assert_eq!(spell_attack_modifier(16, 2), 5);
        // INT 10 (+0) + prof 2 = +2
        assert_eq!(spell_attack_modifier(10, 2), 2);
    }

    #[test]
    fn test_spell_save_dc() {
        // 8 + INT 16 (+3) + prof 2 = 13
        assert_eq!(spell_save_dc(16, 2), 13);
        // 8 + INT 10 (+0) + prof 2 = 10
        assert_eq!(spell_save_dc(10, 2), 10);
    }

    #[test]
    fn test_fire_bolt_rolls_attack_and_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = resolve_fire_bolt(&mut rng, 16, 2, 12);
        match result {
            CastOutcome::FireBolt { attack, damage } => {
                assert!(attack.roll >= 1 && attack.roll <= 20);
                assert_eq!(attack.modifier, 5); // INT 16 (+3) + prof 2
                if attack.hit {
                    assert!(damage >= 1 && damage <= 20); // 1d10, possibly crit
                } else {
                    assert_eq!(damage, 0);
                }
            }
            _ => panic!("Expected FireBolt outcome"),
        }
    }

    #[test]
    fn test_magic_missile_auto_hit() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = resolve_magic_missile(&mut rng);
        match result {
            CastOutcome::MagicMissile { darts, total_damage } => {
                assert_eq!(darts.len(), 3);
                for dart in &darts {
                    assert!(*dart >= 2 && *dart <= 5, "Dart {} out of 1d4+1 range", dart);
                }
                assert_eq!(total_damage, darts.iter().sum::<i32>());
            }
            _ => panic!("Expected MagicMissile outcome"),
        }
    }

    #[test]
    fn test_burning_hands_only_hits_melee_targets() {
        let mut rng = StdRng::seed_from_u64(42);
        let targets = vec![
            test_target("Goblin", 12, 7, 10, 5),   // in range
            test_target("Archer", 13, 10, 14, 30),  // out of range
        ];
        let result = resolve_burning_hands(&mut rng, 16, 2, &targets);
        match result {
            CastOutcome::BurningHands { results, dc, .. } => {
                assert_eq!(dc, 13); // 8 + 3 + 2
                assert_eq!(results.len(), 1, "Only melee target should be affected");
                assert_eq!(results[0].name, "Goblin");
            }
            _ => panic!("Expected BurningHands outcome"),
        }
    }

    #[test]
    fn test_burning_hands_save_half_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        let targets = vec![test_target("Goblin", 12, 7, 10, 5)];
        let result = resolve_burning_hands(&mut rng, 16, 2, &targets);
        match result {
            CastOutcome::BurningHands { total_rolled, half_damage, results, .. } => {
                assert!(total_rolled >= 3 && total_rolled <= 18); // 3d6
                assert_eq!(half_damage, total_rolled / 2);
                let target = &results[0];
                if target.save_result.saved {
                    assert_eq!(target.damage_taken, half_damage);
                } else {
                    assert_eq!(target.damage_taken, total_rolled);
                }
            }
            _ => panic!("Expected BurningHands outcome"),
        }
    }

    #[test]
    fn test_sleep_targets_weakest_first() {
        let mut rng = StdRng::seed_from_u64(100); // Use seed that gives enough HP pool
        let targets = vec![
            test_target("Rat", 10, 3, 10, 5),
            test_target("Goblin", 12, 7, 10, 5),
            test_target("Ogre", 11, 59, 8, 5),
        ];
        let result = resolve_sleep(&mut rng, &targets);
        match result {
            CastOutcome::SleepResult { hp_pool, affected } => {
                assert!(hp_pool >= 5 && hp_pool <= 40); // 5d8
                // Rat (3 HP) should be affected first if pool is sufficient
                if !affected.is_empty() {
                    assert_eq!(affected[0].name, "Rat");
                }
                // Ogre (59 HP) should never be affected by 5d8 (max 40)
                assert!(!affected.iter().any(|t| t.name == "Ogre"));
            }
            _ => panic!("Expected SleepResult outcome"),
        }
    }

    #[test]
    fn test_shield_gives_5_ac() {
        let result = resolve_shield();
        match result {
            CastOutcome::ShieldCast { ac_bonus } => assert_eq!(ac_bonus, 5),
            _ => panic!("Expected ShieldCast outcome"),
        }
    }

    #[test]
    fn test_consume_spell_slot_cantrip() {
        let mut slots = vec![2];
        assert!(consume_spell_slot(0, &mut slots));
        assert_eq!(slots, vec![2]); // cantrips don't consume
    }

    #[test]
    fn test_consume_spell_slot_level1() {
        let mut slots = vec![2];
        assert!(consume_spell_slot(1, &mut slots));
        assert_eq!(slots, vec![1]);
        assert!(consume_spell_slot(1, &mut slots));
        assert_eq!(slots, vec![0]);
        assert!(!consume_spell_slot(1, &mut slots)); // no slots left
    }

    #[test]
    fn test_consume_spell_slot_no_slots_at_level() {
        let mut slots: Vec<i32> = Vec::new();
        assert!(!consume_spell_slot(1, &mut slots));
    }

    #[test]
    fn test_spell_definitions_count() {
        assert_eq!(SPELLS.len(), 6);
        assert_eq!(SPELLS.iter().filter(|s| s.level == 0).count(), 2); // cantrips
        assert_eq!(SPELLS.iter().filter(|s| s.level == 1).count(), 4); // 1st level
    }

    #[test]
    fn test_fire_bolt_is_cantrip() {
        let spell = find_spell("Fire Bolt").unwrap();
        assert_eq!(spell.level, 0);
        assert_eq!(spell.casting, CastingMode::SpellAttack);
    }

    #[test]
    fn test_prestidigitation_is_flavor() {
        let spell = find_spell("Prestidigitation").unwrap();
        assert_eq!(spell.level, 0);
        assert_eq!(spell.casting, CastingMode::Flavor);
    }

    #[test]
    fn test_format_known_spells_wizard() {
        let known = vec![
            "Fire Bolt".to_string(),
            "Prestidigitation".to_string(),
            "Magic Missile".to_string(),
            "Burning Hands".to_string(),
            "Sleep".to_string(),
            "Shield".to_string(),
        ];
        let slots_remaining = vec![2];
        let slots_max = vec![2];

        let lines = format_known_spells(&known, &slots_remaining, &slots_max);
        let text = lines.join("\n");

        assert!(text.contains("Known Spells"));
        assert!(text.contains("Cantrips (at will)"));
        assert!(text.contains("Fire Bolt"));
        assert!(text.contains("Prestidigitation"));
        assert!(text.contains("Level 1 Spells"));
        assert!(text.contains("Magic Missile"));
        assert!(text.contains("Spell Slots"));
        assert!(text.contains("Level 1: 2/2"));
    }

    #[test]
    fn test_format_known_spells_empty() {
        let lines = format_known_spells(&[], &[], &[]);
        assert_eq!(lines, vec!["You don't know any spells."]);
    }

    #[test]
    fn test_format_known_spells_after_slot_use() {
        let known = vec![
            "Fire Bolt".to_string(),
            "Magic Missile".to_string(),
        ];
        let slots_remaining = vec![1];
        let slots_max = vec![2];

        let lines = format_known_spells(&known, &slots_remaining, &slots_max);
        let text = lines.join("\n");

        assert!(text.contains("Level 1: 1/2"));
    }
}
