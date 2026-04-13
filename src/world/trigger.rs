// jurnalis-engine/src/world/trigger.rs
use rand::Rng;
use std::collections::HashMap;
use crate::types::{TriggerId, LocationId, Skill, Ability};
use crate::state::{Trigger, TriggerType};

const TRAP_SUCCESS: &[&str] = &[
    "You notice the tripwire just in time and step over it carefully.",
    "Your keen eye spots a pressure plate in the floor. You avoid it.",
    "You sense something is off and stop before triggering the trap.",
];

const TRAP_FAILURE: &[&str] = &[
    "A dart shoots from the wall, grazing your arm!",
    "The floor gives way slightly and a blade swings past. Too close!",
    "A burst of noxious gas fills the air. You cough and stumble.",
];

const HIDDEN_SUCCESS: &[&str] = &[
    "You notice a concealed door behind a loose stone.",
    "A careful search reveals a hidden compartment.",
    "Your investigation uncovers a secret passage.",
];

const HIDDEN_FAILURE: &[&str] = &[
    "Nothing unusual catches your attention.",
    "You search but find nothing of interest.",
    "The area appears unremarkable.",
];

pub fn generate_triggers(
    rng: &mut impl Rng,
    location_ids: &[LocationId],
    trigger_count: usize,
) -> HashMap<TriggerId, Trigger> {
    let mut triggers = HashMap::new();
    if location_ids.is_empty() {
        return triggers;
    }

    for i in 0..trigger_count {
        let id = i as TriggerId;
        let location = location_ids[rng.gen_range(0..location_ids.len())];
        let dc = rng.gen_range(10..=18);

        let (trigger_type, success_text, failure_text, damage_on_failure) = match rng.gen_range(0..3) {
            0 => {
                // Trap — DEX save, 1d6 damage on failure
                let damage = rng.gen_range(1..=6);
                (
                    TriggerType::SavingThrow(Ability::Dexterity),
                    TRAP_SUCCESS[rng.gen_range(0..TRAP_SUCCESS.len())].to_string(),
                    TRAP_FAILURE[rng.gen_range(0..TRAP_FAILURE.len())].to_string(),
                    damage,
                )
            }
            1 => {
                // Hidden feature — Perception/Investigation check
                let skill = if rng.gen_bool(0.5) { Skill::Perception } else { Skill::Investigation };
                (
                    TriggerType::SkillCheck(skill),
                    HIDDEN_SUCCESS[rng.gen_range(0..HIDDEN_SUCCESS.len())].to_string(),
                    HIDDEN_FAILURE[rng.gen_range(0..HIDDEN_FAILURE.len())].to_string(),
                    0,
                )
            }
            _ => {
                // Passive perception
                (
                    TriggerType::PassivePerception,
                    HIDDEN_SUCCESS[rng.gen_range(0..HIDDEN_SUCCESS.len())].to_string(),
                    HIDDEN_FAILURE[rng.gen_range(0..HIDDEN_FAILURE.len())].to_string(),
                    0,
                )
            }
        };

        triggers.insert(id, Trigger {
            id,
            location,
            trigger_type,
            dc,
            success_text,
            failure_text,
            one_shot: true,
            damage_on_failure,
        });
    }

    triggers
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_generates_correct_count() {
        let mut rng = StdRng::seed_from_u64(42);
        let triggers = generate_triggers(&mut rng, &[0, 1, 2], 5);
        assert_eq!(triggers.len(), 5);
    }

    #[test]
    fn test_triggers_in_valid_locations() {
        let mut rng = StdRng::seed_from_u64(42);
        let loc_ids = vec![0, 1, 2];
        let triggers = generate_triggers(&mut rng, &loc_ids, 8);
        for trigger in triggers.values() {
            assert!(loc_ids.contains(&trigger.location));
        }
    }

    #[test]
    fn test_dc_in_valid_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let triggers = generate_triggers(&mut rng, &[0], 20);
        for trigger in triggers.values() {
            assert!(trigger.dc >= 10 && trigger.dc <= 18, "DC {} out of range", trigger.dc);
        }
    }

    #[test]
    fn test_trap_triggers_have_nonzero_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        // Generate many triggers to ensure we get some traps (SavingThrow type)
        let triggers = generate_triggers(&mut rng, &[0, 1, 2], 30);
        let traps: Vec<_> = triggers.values()
            .filter(|t| matches!(t.trigger_type, TriggerType::SavingThrow(_)))
            .collect();
        assert!(!traps.is_empty(), "Should generate at least one trap trigger");
        for trap in &traps {
            assert!(trap.damage_on_failure >= 1 && trap.damage_on_failure <= 6,
                "Trap damage {} should be 1d6 (1-6)", trap.damage_on_failure);
        }
    }

    #[test]
    fn test_non_trap_triggers_have_zero_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        let triggers = generate_triggers(&mut rng, &[0, 1, 2], 30);
        let non_traps: Vec<_> = triggers.values()
            .filter(|t| !matches!(t.trigger_type, TriggerType::SavingThrow(_)))
            .collect();
        assert!(!non_traps.is_empty(), "Should generate at least one non-trap trigger");
        for trigger in &non_traps {
            assert_eq!(trigger.damage_on_failure, 0,
                "Non-trap trigger should have 0 damage, got {}", trigger.damage_on_failure);
        }
    }
}
