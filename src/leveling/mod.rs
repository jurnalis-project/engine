// jurnalis-engine/src/leveling/mod.rs
// Leveling and XP progression per SRD 5.1.
//
// Dependencies: types.rs, state/, character/. This module does NOT depend on
// combat/, narration/, parser/, world/, or equipment/. Cross-module wiring
// (combat-victory XP awards, objective bonuses, narration) lives in lib.rs.

use crate::character::class::{self, Class};
use crate::character::Character;
use crate::types::Ability;

/// Hard cap on character level per SRD.
pub const LEVEL_CAP: u32 = 20;

/// Levels at which a character earns an Ability Score Improvement (or feat).
/// Per SRD core classes (the universal subset). Class-specific extras are
/// handled by `class_extra_asi_levels`.
pub const ASI_LEVELS: &[u32] = &[4, 8, 12, 16, 19];

/// Extra ASI levels specific to a class. Returns `&[6, 14]` for Fighter,
/// `&[10]` for Rogue, and `&[]` for all other classes.
pub fn class_extra_asi_levels(class: Class) -> &'static [u32] {
    match class {
        Class::Fighter => &[6, 14],
        Class::Rogue   => &[10],
        _              => &[],
    }
}

/// Flat XP bonus awarded when an objective (DefeatNpc or FindItem) is
/// completed. Stacks on top of monster XP for DefeatNpc objectives.
pub const OBJECTIVE_XP_REWARD: u32 = 100;

/// Cumulative XP required to reach each level (1..=20). Index 0 = level 1.
/// Pulled directly from the SRD Character Advancement table.
const XP_THRESHOLDS: [u32; 20] = [
    0,        // 1
    300,      // 2
    900,      // 3
    2_700,    // 4
    6_500,    // 5
    14_000,   // 6
    23_000,   // 7
    34_000,   // 8
    48_000,   // 9
    64_000,   // 10
    85_000,   // 11
    100_000,  // 12
    120_000,  // 13
    140_000,  // 14
    165_000,  // 15
    195_000,  // 16
    225_000,  // 17
    265_000,  // 18
    305_000,  // 19
    355_000,  // 20
];

/// XP required to reach the given level. Returns 0 for level 0 or 1.
/// Returns `u32::MAX` for any level above the cap (functionally
/// unreachable, used as a sentinel for "no further leveling").
pub fn xp_for_level(level: u32) -> u32 {
    if level == 0 {
        return 0;
    }
    if level > LEVEL_CAP {
        return u32::MAX;
    }
    XP_THRESHOLDS[(level - 1) as usize]
}

/// XP required to advance from the given current level to the next.
/// Returns `u32::MAX` if already at the cap.
pub fn xp_for_next_level(level: u32) -> u32 {
    if level >= LEVEL_CAP {
        return u32::MAX;
    }
    xp_for_level(level + 1)
}

/// Highest level whose threshold is `<= xp`. Capped at `LEVEL_CAP`.
pub fn level_for_xp(xp: u32) -> u32 {
    let mut level = 1u32;
    for (i, &threshold) in XP_THRESHOLDS.iter().enumerate() {
        if xp >= threshold {
            level = (i as u32) + 1;
        } else {
            break;
        }
    }
    level
}

/// SRD CR -> XP reward table. Returns 0 for unknown / unsupported CRs.
///
/// Floating-point CRs are matched with a small epsilon tolerance to handle
/// inexact representations (e.g., 0.125, 0.25, 0.5 round-trip exactly in
/// f32 but the tolerance keeps callers safe).
pub fn xp_for_cr(cr: f32) -> u32 {
    const EPS: f32 = 1e-3;
    const TABLE: &[(f32, u32)] = &[
        (0.0, 10),
        (0.125, 25),
        (0.25, 50),
        (0.5, 100),
        (1.0, 200),
        (2.0, 450),
        (3.0, 700),
        (4.0, 1_100),
        (5.0, 1_800),
        (6.0, 2_300),
        (7.0, 2_900),
        (8.0, 3_900),
        (9.0, 5_000),
        (10.0, 5_900),
    ];
    if !cr.is_finite() {
        return 0;
    }
    for &(table_cr, xp) in TABLE {
        if (cr - table_cr).abs() < EPS {
            return xp;
        }
    }
    0
}

/// Per-level fixed HP gain for a class (SRD "Fixed Hit Points by Class").
/// Value is the hit-die-derived constant; the caller adds the CON modifier.
fn fixed_hp_per_level(class: Class) -> i32 {
    // Matches existing `calculate_hp` formula: (hit_die / 2) + 1
    (class.hit_die() as i32 / 2) + 1
}

/// SRD full-caster spell-slot table (Bard, Cleric, Druid, Sorcerer, Wizard).
/// Returns slots for spell levels 1..=9 in a vector indexed from 0.
/// For non-caster classes returns an empty vector.
///
/// For levels above the cap, returns the level-20 row.
pub fn full_caster_spell_slots(level: u32) -> Vec<i32> {
    // Each row is [lvl1, lvl2, lvl3, lvl4, lvl5, lvl6, lvl7, lvl8, lvl9]
    // truncated to the highest non-zero entry to keep `Vec` sizes minimal.
    const TABLE: &[[i32; 9]] = &[
        [2, 0, 0, 0, 0, 0, 0, 0, 0],   // 1
        [3, 0, 0, 0, 0, 0, 0, 0, 0],   // 2
        [4, 2, 0, 0, 0, 0, 0, 0, 0],   // 3
        [4, 3, 0, 0, 0, 0, 0, 0, 0],   // 4
        [4, 3, 2, 0, 0, 0, 0, 0, 0],   // 5
        [4, 3, 3, 0, 0, 0, 0, 0, 0],   // 6
        [4, 3, 3, 1, 0, 0, 0, 0, 0],   // 7
        [4, 3, 3, 2, 0, 0, 0, 0, 0],   // 8
        [4, 3, 3, 3, 1, 0, 0, 0, 0],   // 9
        [4, 3, 3, 3, 2, 0, 0, 0, 0],   // 10
        [4, 3, 3, 3, 2, 1, 0, 0, 0],   // 11
        [4, 3, 3, 3, 2, 1, 0, 0, 0],   // 12
        [4, 3, 3, 3, 2, 1, 1, 0, 0],   // 13
        [4, 3, 3, 3, 2, 1, 1, 0, 0],   // 14
        [4, 3, 3, 3, 2, 1, 1, 1, 0],   // 15
        [4, 3, 3, 3, 2, 1, 1, 1, 0],   // 16
        [4, 3, 3, 3, 2, 1, 1, 1, 1],   // 17
        [4, 3, 3, 3, 3, 1, 1, 1, 1],   // 18
        [4, 3, 3, 3, 3, 2, 1, 1, 1],   // 19
        [4, 3, 3, 3, 3, 2, 2, 1, 1],   // 20
    ];
    if level == 0 {
        return Vec::new();
    }
    let idx = (level.min(LEVEL_CAP) - 1) as usize;
    let row = TABLE[idx];
    // Truncate trailing zeros so Vec lengths grow naturally with level.
    let last_nonzero = row.iter().rposition(|&n| n > 0).map(|i| i + 1).unwrap_or(0);
    row[..last_nonzero].to_vec()
}

/// Backward-compat alias — Wizard uses the full-caster table.
#[inline]
pub fn wizard_spell_slots(level: u32) -> Vec<i32> {
    full_caster_spell_slots(level)
}

/// SRD half-caster spell-slot table (Paladin, Ranger).
/// Spellcasting begins at level 2 for Rangers (engine defers to level 2;
/// method returns empty for level 1 for Ranger). Paladin gains spells at
/// level 1 per 2024 SRD.
///
/// Half-casters use spell levels 1..=5 only.
pub fn half_caster_spell_slots(level: u32) -> Vec<i32> {
    // Each row is [lvl1, lvl2, lvl3, lvl4, lvl5]
    const TABLE: &[[i32; 5]] = &[
        [2, 0, 0, 0, 0],   // 1
        [2, 0, 0, 0, 0],   // 2
        [3, 0, 0, 0, 0],   // 3
        [3, 0, 0, 0, 0],   // 4
        [4, 2, 0, 0, 0],   // 5
        [4, 2, 0, 0, 0],   // 6
        [4, 3, 0, 0, 0],   // 7
        [4, 3, 0, 0, 0],   // 8
        [4, 3, 2, 0, 0],   // 9
        [4, 3, 2, 0, 0],   // 10
        [4, 3, 3, 0, 0],   // 11
        [4, 3, 3, 0, 0],   // 12
        [4, 3, 3, 1, 0],   // 13
        [4, 3, 3, 1, 0],   // 14
        [4, 3, 3, 2, 0],   // 15
        [4, 3, 3, 2, 0],   // 16
        [4, 3, 3, 3, 1],   // 17
        [4, 3, 3, 3, 1],   // 18
        [4, 3, 3, 3, 2],   // 19
        [4, 3, 3, 3, 2],   // 20
    ];
    if level == 0 {
        return Vec::new();
    }
    let idx = (level.min(LEVEL_CAP) - 1) as usize;
    let row = TABLE[idx];
    let last_nonzero = row.iter().rposition(|&n| n > 0).map(|i| i + 1).unwrap_or(0);
    row[..last_nonzero].to_vec()
}

/// SRD Warlock Pact Magic slot table (short-rest recovery).
/// Warlocks have a small pool of high-level slots that all return on a
/// short OR long rest. Slot level equals `(level + 1) / 2` capped at 5.
pub fn warlock_pact_magic_slots(level: u32) -> Vec<i32> {
    // [slots_of_pact_level]
    // Pact slot level: 1 at L1-2, 2 at L3-4, 3 at L5-6, 4 at L7-8, 5 at L9+
    // Number of slots: 1 at L1, 2 at L2+
    if level == 0 {
        return Vec::new();
    }
    let pact_level = ((level + 1) / 2).min(5) as usize; // 1..=5
    let num_slots: i32 = if level >= 2 { 2 } else { 1 };
    // Build a vec with a single non-zero entry at index (pact_level - 1).
    let mut slots = vec![0i32; pact_level];
    slots[pact_level - 1] = num_slots;
    slots
}

/// Result of a single level-up step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelUpReport {
    pub new_level: u32,
    pub hp_gained: i32,
    pub asi_granted: bool,
    pub new_spell_tier_unlocked: Option<usize>, // spell level index (0 = level 1 spell)
    /// Human-readable names of class features unlocked at this level.
    pub new_features: &'static [&'static str],
}

/// Apply a single level-up to the character. Used internally by `award_xp`,
/// exposed for testing. Caller is responsible for ensuring `character.level`
/// has not yet been incremented past the cap.
pub fn perform_level_up(character: &mut Character) -> LevelUpReport {
    let new_level = character.level + 1;

    // 1. HP and current_hp
    let con_mod = Ability::modifier(
        character
            .ability_scores
            .get(&Ability::Constitution)
            .copied()
            .unwrap_or(10),
    );
    let hp_gain = (fixed_hp_per_level(character.class) + con_mod).max(1);
    character.max_hp += hp_gain;
    character.current_hp = (character.current_hp + hp_gain).min(character.max_hp);

    // 2. Hit dice
    if character.hit_dice_remaining < new_level {
        character.hit_dice_remaining += 1;
    }

    // 3. ASI credits — universal levels plus class-specific extras
    let asi_granted = ASI_LEVELS.contains(&new_level)
        || class_extra_asi_levels(character.class).contains(&new_level);
    if asi_granted {
        character.asi_credits += 1;
    }

    // 4. Spell slots — update spell_slots_max and additively credit
    // spell_slots_remaining for changes (level-up is not a long rest).
    //
    // For full/half casters: only newly unlocked slot tiers are added to
    // remaining; already-spent slots in existing tiers are not refilled.
    // This preserves the original design: the player can't "level up" to
    // restore spent slots.
    //
    // For Warlock Pact Magic: the entire slot vector can restructure between
    // levels (pact slot level and count both change). We fully replace
    // spell_slots_remaining with the new max so the player always has the
    // correct pact slots available — this matches the short-rest restore
    // semantic (Warlock effectively always has max pact slots between rests).
    //
    // Classes covered:
    //   Full casters  : Bard, Cleric, Druid, Sorcerer, Wizard
    //   Half casters  : Paladin (level 1+)
    //   Pact Magic    : Warlock
    //   Deferred/none : Ranger (engine defers to L2 via starting_spell_slots)
    //                   Barbarian, Fighter, Monk, Rogue (no slots)
    let mut new_spell_tier_unlocked: Option<usize> = None;
    match character.class {
        Class::Bard | Class::Cleric | Class::Druid
        | Class::Sorcerer | Class::Wizard | Class::Paladin => {
            let new_max = if character.class == Class::Paladin {
                half_caster_spell_slots(new_level)
            } else {
                full_caster_spell_slots(new_level)
            };
            let old_len = character.spell_slots_max.len();
            // Pad remaining defensively.
            while character.spell_slots_remaining.len() < old_len {
                character.spell_slots_remaining.push(0);
            }
            // Credit newly-unlocked tiers only.
            for i in old_len..new_max.len() {
                character.spell_slots_remaining.push(new_max[i]);
            }
            if new_max.len() > old_len {
                new_spell_tier_unlocked = Some(old_len);
            }
            character.spell_slots_max = new_max;
        }
        Class::Warlock => {
            let new_max = warlock_pact_magic_slots(new_level);
            // Warlock slots fully reset to new max on level-up so the player
            // always has access to the correct pact slot tier and count.
            // Short-rest recovery logic in rest/mod.rs is unchanged.
            if new_max != character.spell_slots_max {
                if new_max.len() > character.spell_slots_max.len() {
                    new_spell_tier_unlocked = Some(character.spell_slots_max.len());
                }
                character.spell_slots_remaining = new_max.clone();
                character.spell_slots_max = new_max;
            }
        }
        // Non-casters and Ranger (deferred): no update.
        _ => {}
    }

    // 5. Refresh class-feature flags (newly available features at this level
    // are exposed; existing ones are also refreshed as a courtesy on level-up).
    character.class_features.second_wind_available = true;
    character.class_features.action_surge_available = true;
    character.class_features.arcane_recovery_used_today = false;

    // 5a. Monk: initialize Ki pool at level 2 (Ki points = monk level).
    // On subsequent levels, the pool grows by 1 each level; we set it to
    // the new level count so the character starts the new level fully refreshed.
    if character.class == Class::Monk && new_level >= 2 {
        character.class_features.ki_points_remaining = new_level;
    }

    // 6. Look up class features unlocked at this level.
    let new_features = class::new_class_features_at_level(character.class, new_level);

    // 7. Increment the level last so the report reflects the new level.
    character.level = new_level;

    LevelUpReport {
        new_level,
        hp_gained: hp_gain,
        asi_granted,
        new_spell_tier_unlocked,
        new_features,
    }
}

/// Award XP to the character and apply any resulting level-ups. Returns
/// narration lines describing what changed (XP gained + any level-ups).
///
/// Caller (orchestrator in lib.rs) is responsible for emitting these lines
/// to the player.
///
/// The optional `source` parameter appends a suffix to the XP message,
/// e.g., `Some("from combat")` yields "You gain 100 XP from combat."
pub fn award_xp(character: &mut Character, amount: u32, source: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    if amount == 0 {
        return lines;
    }
    character.xp = character.xp.saturating_add(amount);
    let xp_msg = match source {
        Some(s) => format!("You gain {} XP {}.", amount, s),
        None => format!("You gain {} XP.", amount),
    };
    lines.push(xp_msg);

    while character.level < LEVEL_CAP
        && character.xp >= xp_for_next_level(character.level)
    {
        let report = perform_level_up(character);
        lines.push(format!(
            "*** You reached level {}! HP +{} (now {}/{}). Hit dice: {}/{}. ***",
            report.new_level,
            report.hp_gained,
            character.current_hp,
            character.max_hp,
            character.hit_dice_remaining,
            character.level,
        ));
        if report.asi_granted {
            lines.push(format!(
                "You earn an Ability Score Improvement! (You have {} unspent ASI credit{}.)",
                character.asi_credits,
                if character.asi_credits == 1 { "" } else { "s" },
            ));
        }
        if let Some(tier) = report.new_spell_tier_unlocked {
            lines.push(format!(
                "You unlock level {} spell slots!",
                tier + 1,
            ));
        }
        for feature in report.new_features {
            lines.push(format!("Class feature unlocked: {}.", feature));
        }
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::race::Race;
    use crate::character::{create_character, Character};
    use crate::types::{Ability, Skill};
    use std::collections::HashMap;

    fn fighter_con13() -> Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 15);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 12);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);
        create_character(
            "Test".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            Vec::<Skill>::new(),
        )
    }

    fn wizard_con13() -> Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 8);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 15);
        scores.insert(Ability::Wisdom, 12);
        scores.insert(Ability::Charisma, 10);
        create_character(
            "Wiz".to_string(),
            Race::Human,
            Class::Wizard,
            scores,
            Vec::<Skill>::new(),
        )
    }

    // ---- XP threshold table ----

    #[test]
    fn xp_for_level_known_thresholds() {
        assert_eq!(xp_for_level(1), 0);
        assert_eq!(xp_for_level(2), 300);
        assert_eq!(xp_for_level(3), 900);
        assert_eq!(xp_for_level(4), 2_700);
        assert_eq!(xp_for_level(5), 6_500);
        assert_eq!(xp_for_level(20), 355_000);
    }

    #[test]
    fn xp_for_level_zero_and_above_cap() {
        assert_eq!(xp_for_level(0), 0);
        assert_eq!(xp_for_level(21), u32::MAX);
        assert_eq!(xp_for_level(100), u32::MAX);
    }

    #[test]
    fn xp_for_next_level_returns_threshold_for_next() {
        assert_eq!(xp_for_next_level(1), 300);
        assert_eq!(xp_for_next_level(4), 6_500);
        assert_eq!(xp_for_next_level(19), 355_000);
        assert_eq!(xp_for_next_level(20), u32::MAX);
    }

    #[test]
    fn level_for_xp_lookup() {
        assert_eq!(level_for_xp(0), 1);
        assert_eq!(level_for_xp(299), 1);
        assert_eq!(level_for_xp(300), 2);
        assert_eq!(level_for_xp(899), 2);
        assert_eq!(level_for_xp(900), 3);
        assert_eq!(level_for_xp(6_499), 4);
        assert_eq!(level_for_xp(6_500), 5);
        assert_eq!(level_for_xp(355_000), 20);
        assert_eq!(level_for_xp(u32::MAX), 20);
    }

    // ---- CR -> XP table ----

    #[test]
    fn cr_to_xp_known_values() {
        assert_eq!(xp_for_cr(0.0), 10);
        assert_eq!(xp_for_cr(0.125), 25);
        assert_eq!(xp_for_cr(0.25), 50);
        assert_eq!(xp_for_cr(0.5), 100);
        assert_eq!(xp_for_cr(1.0), 200);
        assert_eq!(xp_for_cr(2.0), 450);
        assert_eq!(xp_for_cr(10.0), 5_900);
    }

    #[test]
    fn cr_to_xp_unknown_returns_zero() {
        assert_eq!(xp_for_cr(1.7), 0);
        assert_eq!(xp_for_cr(-1.0), 0);
        assert_eq!(xp_for_cr(f32::NAN), 0);
        assert_eq!(xp_for_cr(f32::INFINITY), 0);
    }

    // ---- Wizard spell slots ----

    #[test]
    fn wizard_slots_at_known_levels() {
        assert_eq!(wizard_spell_slots(1), vec![2]);
        assert_eq!(wizard_spell_slots(2), vec![3]);
        assert_eq!(wizard_spell_slots(3), vec![4, 2]);
        assert_eq!(wizard_spell_slots(5), vec![4, 3, 2]);
        assert_eq!(wizard_spell_slots(11), vec![4, 3, 3, 3, 2, 1]);
        assert_eq!(wizard_spell_slots(17), vec![4, 3, 3, 3, 2, 1, 1, 1, 1]);
        assert_eq!(wizard_spell_slots(20), vec![4, 3, 3, 3, 3, 2, 2, 1, 1]);
    }

    #[test]
    fn wizard_slots_zero_level_empty() {
        assert!(wizard_spell_slots(0).is_empty());
    }

    // ---- Level-up: HP and hit dice ----

    #[test]
    fn level_up_fighter_increases_hp_and_hit_dice() {
        let mut c = fighter_con13();
        let starting_hp = c.max_hp;
        let starting_hd = c.hit_dice_remaining;
        let report = perform_level_up(&mut c);
        // Human +1 to all -> CON 14 (mod +2). Fighter d10 -> +6 + 2 = +8.
        assert_eq!(report.hp_gained, 8);
        assert_eq!(c.max_hp, starting_hp + 8);
        assert_eq!(c.current_hp, c.max_hp); // partial heal up to new max
        assert_eq!(c.hit_dice_remaining, starting_hd + 1);
        assert_eq!(c.level, 2);
        assert!(!report.asi_granted);
    }

    #[test]
    fn level_up_wizard_increases_hp() {
        let mut c = wizard_con13();
        // Human +1 -> CON 14 (mod +2). Wizard d6 -> +4 + 2 = +6.
        let report = perform_level_up(&mut c);
        assert_eq!(report.hp_gained, 6);
    }

    #[test]
    fn level_up_negative_con_grants_at_least_one_hp() {
        let mut c = fighter_con13();
        c.ability_scores.insert(Ability::Constitution, 4); // CON mod -3
        // Fighter d10 -> +6 + (-3) = +3, still > 1 so OK.
        // Force a worse case: very weak class
        c.class = Class::Wizard;
        // Wizard +4 + (-3) = +1
        let report = perform_level_up(&mut c);
        assert_eq!(report.hp_gained, 1);
    }

    #[test]
    fn level_up_grants_asi_at_level_4() {
        let mut c = fighter_con13();
        // Bump to level 3 first
        c.level = 3;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 4);
        assert!(report.asi_granted);
        assert_eq!(c.asi_credits, 1);
    }

    #[test]
    fn level_up_no_asi_at_non_asi_level() {
        let mut c = fighter_con13();
        for _ in 0..6 {
            // Walk up to level 7; Fighter gets ASI at 4 and 6
            perform_level_up(&mut c);
        }
        assert_eq!(c.level, 7);
        assert_eq!(c.asi_credits, 2); // level 4 (universal) + level 6 (Fighter extra)
    }

    #[test]
    fn level_up_wizard_adds_new_spell_tier() {
        let mut c = wizard_con13();
        // L1 wizard has [2]
        assert_eq!(c.spell_slots_max, vec![2]);
        // Level up to 2: still just one tier but more
        let _ = perform_level_up(&mut c);
        assert_eq!(c.spell_slots_max, vec![3]);
        // Spending one slot to test that additive behavior preserves spent slots
        c.spell_slots_remaining[0] = 1;
        // Level up to 3: gains a tier
        let report = perform_level_up(&mut c);
        assert_eq!(c.spell_slots_max, vec![4, 2]);
        assert_eq!(report.new_spell_tier_unlocked, Some(1));
        // Existing spent slots are NOT topped back up
        assert_eq!(c.spell_slots_remaining[0], 1);
        // New tier is fully populated
        assert_eq!(c.spell_slots_remaining[1], 2);
    }

    #[test]
    fn level_up_fighter_does_not_gain_spell_slots() {
        let mut c = fighter_con13();
        for _ in 0..5 {
            perform_level_up(&mut c);
        }
        assert!(c.spell_slots_max.is_empty());
        assert!(c.spell_slots_remaining.is_empty());
    }

    // ---- award_xp: thresholds and level-up triggering ----

    #[test]
    fn award_xp_below_threshold_no_level_up() {
        let mut c = fighter_con13();
        let lines = award_xp(&mut c, 100, None);
        assert_eq!(c.xp, 100);
        assert_eq!(c.level, 1);
        assert!(lines.iter().any(|l| l.contains("100 XP")));
        assert!(!lines.iter().any(|l| l.contains("level")));
    }

    #[test]
    fn award_xp_zero_amount_is_noop() {
        let mut c = fighter_con13();
        let lines = award_xp(&mut c, 0, None);
        assert!(lines.is_empty());
        assert_eq!(c.xp, 0);
    }

    #[test]
    fn award_xp_crosses_one_threshold() {
        let mut c = fighter_con13();
        let lines = award_xp(&mut c, 300, None);
        assert_eq!(c.level, 2);
        assert!(lines.iter().any(|l| l.contains("level 2")));
    }

    #[test]
    fn award_xp_crosses_multiple_thresholds_in_one_award() {
        let mut c = fighter_con13();
        // 7000 XP — level 1 -> 5 (300/900/2700/6500 thresholds).
        let lines = award_xp(&mut c, 7_000, None);
        assert_eq!(c.level, 5);
        // Should mention each level reached.
        assert!(lines.iter().any(|l| l.contains("level 2")));
        assert!(lines.iter().any(|l| l.contains("level 3")));
        assert!(lines.iter().any(|l| l.contains("level 4")));
        assert!(lines.iter().any(|l| l.contains("level 5")));
    }

    #[test]
    fn award_xp_caps_at_level_20() {
        let mut c = fighter_con13();
        // Bump XP into the stratosphere — level should cap at 20.
        let _ = award_xp(&mut c, 1_000_000, None);
        assert_eq!(c.level, 20);
        // A second huge award changes nothing about level.
        let prior_hp = c.max_hp;
        let prior_hd = c.hit_dice_remaining;
        let prior_asi = c.asi_credits;
        let _ = award_xp(&mut c, 1_000_000, None);
        assert_eq!(c.level, 20);
        assert_eq!(c.max_hp, prior_hp);
        assert_eq!(c.hit_dice_remaining, prior_hd);
        assert_eq!(c.asi_credits, prior_asi);
    }

    #[test]
    fn award_xp_with_source_label() {
        let mut c = fighter_con13();
        let lines = award_xp(&mut c, 100, Some("from combat"));
        assert!(
            lines.iter().any(|l| l.contains("100 XP from combat")),
            "Expected source label in XP message, got: {:?}",
            lines
        );

        let lines2 = award_xp(&mut c, 50, Some("for completing the objective"));
        assert!(
            lines2
                .iter()
                .any(|l| l.contains("50 XP for completing the objective")),
            "Expected objective source label, got: {:?}",
            lines2
        );
    }

    // ---- Class feature announcements on level-up ----

    #[test]
    fn level_up_fighter_reports_new_features_at_level_2() {
        let mut c = fighter_con13();
        // Level 1 -> 2: Fighter gets Action Surge
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 2);
        assert!(
            report.new_features.contains(&"Action Surge"),
            "Fighter L2 should report Action Surge, got {:?}",
            report.new_features,
        );
    }

    #[test]
    fn level_up_fighter_no_features_at_level_7() {
        let mut c = fighter_con13();
        c.level = 6;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 7);
        assert!(
            report.new_features.is_empty(),
            "Fighter L7 should report no new features, got {:?}",
            report.new_features,
        );
    }

    #[test]
    fn award_xp_narrates_class_features_on_level_up() {
        let mut c = fighter_con13();
        // Award enough XP to reach level 2 (threshold = 300)
        let lines = award_xp(&mut c, 300, None);
        assert_eq!(c.level, 2);
        assert!(
            lines.iter().any(|l| l.contains("Action Surge")),
            "Level-up narration should mention Action Surge, got: {:?}",
            lines,
        );
    }

    #[test]
    fn award_xp_narrates_multiple_features_across_multiple_levels() {
        let mut c = fighter_con13();
        // Award enough XP to reach level 5 (threshold = 6500)
        // L2: Action Surge, L3: Martial Archetype, L5: Extra Attack
        let lines = award_xp(&mut c, 6_500, None);
        assert_eq!(c.level, 5);
        assert!(
            lines.iter().any(|l| l.contains("Action Surge")),
            "Should mention Action Surge, got: {:?}",
            lines,
        );
        assert!(
            lines.iter().any(|l| l.contains("Extra Attack")),
            "Should mention Extra Attack, got: {:?}",
            lines,
        );
    }

    // ---- Save/load ----

    #[test]
    fn xp_field_save_load_roundtrip() {
        let mut c = fighter_con13();
        c.xp = 4_321;
        c.asi_credits = 2;
        let json = serde_json::to_string(&c).unwrap();
        let loaded: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.xp, 4_321);
        assert_eq!(loaded.asi_credits, 2);
    }

    #[test]
    fn legacy_save_missing_xp_defaults_to_zero() {
        let c = fighter_con13();
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("xp");
        v.as_object_mut().unwrap().remove("asi_credits");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.xp, 0);
        assert_eq!(loaded.asi_credits, 0);
    }

    // ---- Class-extra ASI levels (Fighter 6/14, Rogue 10) ----

    #[test]
    fn class_extra_asi_levels_fighter_returns_6_and_14() {
        assert_eq!(class_extra_asi_levels(Class::Fighter), &[6, 14]);
    }

    #[test]
    fn class_extra_asi_levels_rogue_returns_10() {
        assert_eq!(class_extra_asi_levels(Class::Rogue), &[10]);
    }

    #[test]
    fn class_extra_asi_levels_other_classes_return_empty() {
        for class in [Class::Wizard, Class::Barbarian, Class::Bard, Class::Cleric,
                      Class::Druid, Class::Monk, Class::Paladin, Class::Ranger,
                      Class::Sorcerer, Class::Warlock] {
            assert!(class_extra_asi_levels(class).is_empty(),
                "{:?} should have no extra ASI levels", class);
        }
    }

    #[test]
    fn fighter_level_6_grants_extra_asi() {
        let mut c = fighter_con13();
        c.level = 5;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 6);
        assert!(report.asi_granted, "Fighter level 6 should grant ASI");
        assert_eq!(c.asi_credits, 1);
    }

    #[test]
    fn fighter_level_14_grants_extra_asi() {
        let mut c = fighter_con13();
        c.level = 13;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 14);
        assert!(report.asi_granted, "Fighter level 14 should grant ASI");
    }

    #[test]
    fn rogue_level_10_grants_extra_asi() {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 10);
        scores.insert(Ability::Dexterity, 15);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 12);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);
        let mut c = create_character("Rogue".to_string(), Race::Human,
            Class::Rogue, scores, Vec::<Skill>::new());
        c.level = 9;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 10);
        assert!(report.asi_granted, "Rogue level 10 should grant ASI");
    }

    #[test]
    fn non_fighter_non_rogue_level_6_no_extra_asi() {
        let mut c = wizard_con13();
        c.level = 5;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 6);
        assert!(!report.asi_granted, "Wizard level 6 should not grant ASI");
    }

    #[test]
    fn non_fighter_non_rogue_level_10_no_extra_asi() {
        let mut c = wizard_con13();
        c.level = 9;
        let report = perform_level_up(&mut c);
        assert_eq!(c.level, 10);
        assert!(!report.asi_granted, "Wizard level 10 should not grant ASI");
    }

    // ---- Spell slot progression for non-Wizard casters (fix for issue #302) ----

    fn make_caster(class: Class) -> crate::character::Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 8);
        scores.insert(Ability::Dexterity, 12);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 10);
        scores.insert(Ability::Wisdom, 14);
        scores.insert(Ability::Charisma, 15);
        create_character("Caster".to_string(), crate::character::race::Race::Human,
            class, scores, Vec::<crate::types::Skill>::new())
    }

    #[test]
    fn cleric_spell_slots_max_updates_on_level_up() {
        let mut c = make_caster(Class::Cleric);
        assert_eq!(c.spell_slots_max, vec![2], "Cleric L1 should have [2]");
        perform_level_up(&mut c); // -> L2
        assert_eq!(c.spell_slots_max, vec![3], "Cleric L2 should have [3]");
        perform_level_up(&mut c); // -> L3
        assert_eq!(c.spell_slots_max, vec![4, 2], "Cleric L3 should have [4, 2]");
    }

    #[test]
    fn sorcerer_spell_slots_max_updates_on_level_up() {
        let mut c = make_caster(Class::Sorcerer);
        perform_level_up(&mut c); // -> L2
        perform_level_up(&mut c); // -> L3
        assert_eq!(c.spell_slots_max, vec![4, 2], "Sorcerer L3 should have [4, 2]");
        perform_level_up(&mut c); // -> L4
        perform_level_up(&mut c); // -> L5
        assert_eq!(c.spell_slots_max, vec![4, 3, 2], "Sorcerer L5 should have [4, 3, 2]");
    }

    #[test]
    fn bard_spell_slots_max_updates_on_level_up() {
        let mut c = make_caster(Class::Bard);
        perform_level_up(&mut c); // -> L2
        perform_level_up(&mut c); // -> L3
        assert_eq!(c.spell_slots_max, vec![4, 2], "Bard L3 should have [4, 2]");
    }

    #[test]
    fn druid_spell_slots_max_updates_on_level_up() {
        let mut c = make_caster(Class::Druid);
        perform_level_up(&mut c); // -> L2
        perform_level_up(&mut c); // -> L3
        assert_eq!(c.spell_slots_max, vec![4, 2], "Druid L3 should have [4, 2]");
    }

    #[test]
    fn paladin_spell_slots_max_updates_on_level_up() {
        // Paladin is a half caster. L5: [4, 2] per the half-caster table.
        let mut c = make_caster(Class::Paladin);
        for _ in 0..4 { perform_level_up(&mut c); } // L1 -> L5
        assert_eq!(c.spell_slots_max, vec![4, 2], "Paladin L5 should have [4, 2]");
    }

    #[test]
    fn warlock_spell_slots_max_updates_on_level_up() {
        // Warlock L1: [1] (1 L1 pact slot). L2: [2] (2 L1 slots). L3: [0, 2] (2 L2 slots).
        let mut c = make_caster(Class::Warlock);
        assert_eq!(c.spell_slots_max, vec![1], "Warlock L1 should have [1]");
        perform_level_up(&mut c); // -> L2
        assert_eq!(c.spell_slots_max, vec![2], "Warlock L2 should have [2]");
        perform_level_up(&mut c); // -> L3
        assert_eq!(c.spell_slots_max, vec![0, 2], "Warlock L3 should have [0, 2]");
    }

    #[test]
    fn warlock_spell_slots_remaining_set_to_new_max_on_level_up() {
        // Warlock remaining should reset to new max on level-up (pact magic refresh).
        let mut c = make_caster(Class::Warlock);
        // Spend the single L1 slot.
        c.spell_slots_remaining = vec![0];
        perform_level_up(&mut c); // -> L2: max becomes [2]
        assert_eq!(c.spell_slots_remaining, vec![2],
            "Warlock remaining should reflect new L2 max [2] after level-up");
    }

    #[test]
    fn full_caster_spell_slots_table_spot_checks() {
        assert_eq!(full_caster_spell_slots(1),  vec![2]);
        assert_eq!(full_caster_spell_slots(3),  vec![4, 2]);
        assert_eq!(full_caster_spell_slots(5),  vec![4, 3, 2]);
        assert_eq!(full_caster_spell_slots(17), vec![4, 3, 3, 3, 2, 1, 1, 1, 1]);
        assert_eq!(full_caster_spell_slots(20), vec![4, 3, 3, 3, 3, 2, 2, 1, 1]);
    }

    #[test]
    fn half_caster_spell_slots_table_spot_checks() {
        // Half casters gain spells at L1 (Paladin) or L2 (Ranger deferred).
        assert_eq!(half_caster_spell_slots(1),  vec![2]);
        assert_eq!(half_caster_spell_slots(5),  vec![4, 2]);
        assert_eq!(half_caster_spell_slots(9),  vec![4, 3, 2]);
        assert_eq!(half_caster_spell_slots(20), vec![4, 3, 3, 3, 2]);
    }

    #[test]
    fn warlock_pact_magic_table_spot_checks() {
        assert_eq!(warlock_pact_magic_slots(1), vec![1]);
        assert_eq!(warlock_pact_magic_slots(2), vec![2]);
        assert_eq!(warlock_pact_magic_slots(3), vec![0, 2]);
        assert_eq!(warlock_pact_magic_slots(5), vec![0, 0, 2]);
        assert_eq!(warlock_pact_magic_slots(9), vec![0, 0, 0, 0, 2]); // L5 pact slots
    }
}
