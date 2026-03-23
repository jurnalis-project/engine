use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};
use super::dice::roll_d20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub roll: i32,
    pub modifier: i32,
    pub total: i32,
    pub dc: i32,
    pub success: bool,
    pub natural_20: bool,
    pub natural_1: bool,
}

pub fn ability_check(
    rng: &mut impl Rng,
    ability_score: i32,
    proficiency_bonus: i32,
    is_proficient: bool,
    dc: i32,
    advantage: bool,
    disadvantage: bool,
) -> CheckResult {
    let roll1 = roll_d20(rng);
    let roll2 = roll_d20(rng);

    let roll = if advantage && !disadvantage {
        roll1.max(roll2)
    } else if disadvantage && !advantage {
        roll1.min(roll2)
    } else {
        roll1
    };

    let modifier = Ability::modifier(ability_score)
        + if is_proficient { proficiency_bonus } else { 0 };
    let total = roll + modifier;

    CheckResult {
        roll,
        modifier,
        total,
        dc,
        success: total >= dc,
        natural_20: roll == 20,
        natural_1: roll == 1,
    }
}

pub fn skill_check(
    rng: &mut impl Rng,
    skill: Skill,
    ability_scores: &std::collections::HashMap<Ability, i32>,
    proficiencies: &[Skill],
    proficiency_bonus: i32,
    dc: i32,
    advantage: bool,
    disadvantage: bool,
) -> CheckResult {
    let ability = skill.ability();
    let ability_score = ability_scores.get(&ability).copied().unwrap_or(10);
    let is_proficient = proficiencies.contains(&skill);
    ability_check(rng, ability_score, proficiency_bonus, is_proficient, dc, advantage, disadvantage)
}

pub fn passive_check(ability_score: i32, proficiency_bonus: i32, is_proficient: bool) -> i32 {
    10 + Ability::modifier(ability_score) + if is_proficient { proficiency_bonus } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;

    #[test]
    fn test_ability_check_basic() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = ability_check(&mut rng, 14, 2, true, 15, false, false);
        assert_eq!(result.modifier, 4); // +2 ability mod + 2 proficiency
        assert_eq!(result.total, result.roll + result.modifier);
        assert_eq!(result.success, result.total >= 15);
    }

    #[test]
    fn test_ability_check_not_proficient() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = ability_check(&mut rng, 14, 2, false, 10, false, false);
        assert_eq!(result.modifier, 2); // +2 ability mod, no proficiency
    }

    #[test]
    fn test_advantage_takes_higher() {
        let mut adv_total = 0;
        let mut normal_total = 0;
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let adv = ability_check(&mut rng, 10, 0, false, 10, true, false);
            let mut rng = StdRng::seed_from_u64(seed);
            let norm = ability_check(&mut rng, 10, 0, false, 10, false, false);
            adv_total += adv.roll;
            normal_total += norm.roll;
        }
        assert!(adv_total > normal_total, "Advantage should produce higher average rolls");
    }

    #[test]
    fn test_disadvantage_takes_lower() {
        let mut dis_total = 0;
        let mut normal_total = 0;
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let dis = ability_check(&mut rng, 10, 0, false, 10, false, true);
            let mut rng = StdRng::seed_from_u64(seed);
            let norm = ability_check(&mut rng, 10, 0, false, 10, false, false);
            dis_total += dis.roll;
            normal_total += norm.roll;
        }
        assert!(dis_total < normal_total, "Disadvantage should produce lower average rolls");
    }

    #[test]
    fn test_advantage_and_disadvantage_cancel() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        let both = ability_check(&mut rng1, 10, 0, false, 10, true, true);
        let neither = ability_check(&mut rng2, 10, 0, false, 10, false, false);
        assert_eq!(both.roll, neither.roll, "Advantage + disadvantage should cancel");
    }

    #[test]
    fn test_skill_check_uses_correct_ability() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut scores = HashMap::new();
        scores.insert(Ability::Dexterity, 16);
        scores.insert(Ability::Strength, 8);
        let proficiencies = vec![Skill::Stealth];

        let result = skill_check(&mut rng, Skill::Stealth, &scores, &proficiencies, 2, 10, false, false);
        assert_eq!(result.modifier, 5); // +3 DEX mod + 2 proficiency
    }

    #[test]
    fn test_passive_check() {
        assert_eq!(passive_check(14, 2, true), 14);  // 10 + 2 + 2
        assert_eq!(passive_check(14, 2, false), 12); // 10 + 2
        assert_eq!(passive_check(8, 2, true), 11);   // 10 + (-1) + 2
    }
}
