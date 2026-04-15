// Conditions system: status effects with mechanical impact
use serde::{Deserialize, Serialize};
use crate::types::Ability;

/// SRD condition types. Covers the full SRD glossary.
///
/// `Exhaustion` is listed here for name/narration parity, but exhaustion level is
/// tracked on `Character.exhaustion` (u32, 0..=6) rather than as an `ActiveCondition`
/// entry. See `docs/specs/conditions-system.md` for the unified 2024 SRD formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConditionType {
    Blinded,
    Charmed,
    Deafened,
    Exhaustion,
    Frightened,
    Grappled,
    Incapacitated,
    Invisible,
    Paralyzed,
    Petrified,
    Poisoned,
    Prone,
    Restrained,
    Stunned,
    Unconscious,
}

impl ConditionType {
    /// Display name for narration (lowercase, used in "You are {name}!" templates).
    pub fn name(&self) -> &'static str {
        match self {
            ConditionType::Blinded => "blinded",
            ConditionType::Charmed => "charmed",
            ConditionType::Deafened => "deafened",
            ConditionType::Exhaustion => "exhaustion",
            ConditionType::Frightened => "frightened",
            ConditionType::Grappled => "grappled",
            ConditionType::Incapacitated => "incapacitated",
            ConditionType::Invisible => "invisible",
            ConditionType::Paralyzed => "paralyzed",
            ConditionType::Petrified => "petrified",
            ConditionType::Poisoned => "poisoned",
            ConditionType::Prone => "prone",
            ConditionType::Restrained => "restrained",
            ConditionType::Stunned => "stunned",
            ConditionType::Unconscious => "unconscious",
        }
    }
}

/// How long a condition lasts
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConditionDuration {
    /// Fixed number of rounds remaining
    Rounds(u32),
    /// Ends when target succeeds on save
    SaveEnds { ability: Ability, dc: i32 },
    /// Until explicitly removed (rest, magic, etc.)
    Permanent,
}

/// An active condition instance on a combatant
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCondition {
    pub condition: ConditionType,
    pub duration: ConditionDuration,
    pub source: Option<String>,
}

impl ActiveCondition {
    pub fn new(condition: ConditionType, duration: ConditionDuration) -> Self {
        Self { condition, duration, source: None }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}

/// Advantage state from conditions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionAdvantage {
    Advantage,
    Disadvantage,
    Normal,
}

/// Check if a condition list contains a specific condition
pub fn has_condition(conditions: &[ActiveCondition], condition: ConditionType) -> bool {
    conditions.iter().any(|c| c.condition == condition)
}

/// Get attack roll advantage/disadvantage from conditions
/// Returns Some(true) for advantage, Some(false) for disadvantage, None for no effect
pub fn get_attack_advantage(conditions: &[ActiveCondition]) -> Option<bool> {
    let mut has_disadvantage = false;

    // Poisoned and Blinded impose disadvantage on attacks
    if has_condition(conditions, ConditionType::Poisoned)
        || has_condition(conditions, ConditionType::Blinded)
    {
        has_disadvantage = true;
    }

    // Prone imposes disadvantage on attacks
    if has_condition(conditions, ConditionType::Prone) {
        has_disadvantage = true;
    }

    // Stunned/Paralyzed are incapacitated - can't attack at all (handled separately)

    if has_disadvantage {
        Some(false) // disadvantage
    } else {
        None
    }
}

/// Get defense advantage/disadvantage from conditions
/// attacker_conditions: conditions on the one making the attack
/// defender_conditions: conditions on the one being attacked
pub fn get_defense_advantage(
    _attacker_conditions: &[ActiveCondition],
    defender_conditions: &[ActiveCondition],
) -> Option<bool> {
    // Attacking a prone target: advantage if within 5ft, disadvantage otherwise
    if has_condition(defender_conditions, ConditionType::Prone) {
        // Within 5ft check is done at call site, default to advantage for now
        return Some(true);
    }

    // Attacking a stunned or paralyzed target: advantage
    if has_condition(defender_conditions, ConditionType::Stunned)
        || has_condition(defender_conditions, ConditionType::Paralyzed)
    {
        return Some(true);
    }

    // Attacking a blinded target: advantage (they can't see you)
    if has_condition(defender_conditions, ConditionType::Blinded) {
        return Some(true);
    }

    None
}

/// Check if conditions cause auto-fail on a saving throw
pub fn get_save_auto_fail(conditions: &[ActiveCondition], ability: Ability) -> bool {
    // Stunned and Paralyzed auto-fail STR and DEX saves
    let is_incapacitated = has_condition(conditions, ConditionType::Stunned)
        || has_condition(conditions, ConditionType::Paralyzed);

    if is_incapacitated && (ability == Ability::Strength || ability == Ability::Dexterity) {
        return true;
    }

    false
}

/// Check if the combatant can take actions
pub fn can_take_actions(conditions: &[ActiveCondition]) -> bool {
    // Stunned and Paralyzed are incapacitated
    !has_condition(conditions, ConditionType::Stunned)
        && !has_condition(conditions, ConditionType::Paralyzed)
}

/// Check if the combatant can take reactions
pub fn can_take_reactions(conditions: &[ActiveCondition]) -> bool {
    // Same as actions for MVP
    can_take_actions(conditions)
}

/// Get movement speed multiplier
pub fn get_speed_multiplier(conditions: &[ActiveCondition]) -> f32 {
    // Prone: crawling costs 2ft per 1ft moved (0.5x speed effectively when standing up)
    if has_condition(conditions, ConditionType::Prone) {
        0.5
    } else {
        1.0
    }
}

/// Check if attacks against this target are automatic critical hits
pub fn is_auto_crit_target(conditions: &[ActiveCondition]) -> bool {
    // Paralyzed: attacks within 5ft are auto-crits
    has_condition(conditions, ConditionType::Paralyzed)
}

/// Decrement round-based durations, returning true if condition expired
pub fn tick_duration(condition: &mut ActiveCondition) -> bool {
    match &mut condition.duration {
        ConditionDuration::Rounds(remaining) => {
            if *remaining > 0 {
                *remaining -= 1;
            }
            *remaining == 0
        }
        _ => false, // SaveEnds and Permanent don't tick
    }
}

/// Get the save ability and DC for a condition, if applicable. Most conditions
/// have no intrinsic save -- they are applied and cleared by specific effects or
/// mechanics (grapples end on escape attempts, frightened ends when source leaves
/// LoS, etc.). Callers should override the default DC via `ConditionDuration::SaveEnds`
/// for source-specific DCs.
pub fn get_save_for_condition(condition: ConditionType) -> Option<(Ability, i32)> {
    match condition {
        ConditionType::Poisoned => Some((Ability::Constitution, 10)),
        // All other conditions use fixed duration, save-ends with source DC,
        // or special recovery (escape grapple, stand up from prone, etc.).
        ConditionType::Blinded
        | ConditionType::Charmed
        | ConditionType::Deafened
        | ConditionType::Exhaustion
        | ConditionType::Frightened
        | ConditionType::Grappled
        | ConditionType::Incapacitated
        | ConditionType::Invisible
        | ConditionType::Paralyzed
        | ConditionType::Petrified
        | ConditionType::Prone
        | ConditionType::Restrained
        | ConditionType::Stunned
        | ConditionType::Unconscious => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_condition_type_name() {
        assert_eq!(ConditionType::Poisoned.name(), "poisoned");
        assert_eq!(ConditionType::Stunned.name(), "stunned");
        assert_eq!(ConditionType::Paralyzed.name(), "paralyzed");
    }

    #[test]
    fn test_new_condition_type_names() {
        // All 10 new conditions should have lowercase display names for narration.
        assert_eq!(ConditionType::Charmed.name(), "charmed");
        assert_eq!(ConditionType::Deafened.name(), "deafened");
        assert_eq!(ConditionType::Frightened.name(), "frightened");
        assert_eq!(ConditionType::Grappled.name(), "grappled");
        assert_eq!(ConditionType::Incapacitated.name(), "incapacitated");
        assert_eq!(ConditionType::Invisible.name(), "invisible");
        assert_eq!(ConditionType::Petrified.name(), "petrified");
        assert_eq!(ConditionType::Restrained.name(), "restrained");
        assert_eq!(ConditionType::Unconscious.name(), "unconscious");
        // Exhaustion has a display name but is otherwise tracked as u32 on Character.
        assert_eq!(ConditionType::Exhaustion.name(), "exhaustion");
    }

    #[test]
    fn test_has_condition() {
        let conditions = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        ];
        assert!(has_condition(&conditions, ConditionType::Poisoned));
        assert!(!has_condition(&conditions, ConditionType::Stunned));
    }

    #[test]
    fn test_poisoned_imposes_attack_disadvantage() {
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert_eq!(get_attack_advantage(&poisoned), Some(false));
    }

    #[test]
    fn test_blinded_imposes_attack_disadvantage() {
        let blinded = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(1)),
        ];
        assert_eq!(get_attack_advantage(&blinded), Some(false));
    }

    #[test]
    fn test_prone_imposes_attack_disadvantage() {
        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];
        assert_eq!(get_attack_advantage(&prone), Some(false));
    }

    #[test]
    fn test_no_condition_no_attack_effect() {
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_attack_advantage(&empty), None);
    }

    #[test]
    fn test_stunned_grants_defense_advantage() {
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &stunned), Some(true));
    }

    #[test]
    fn test_paralyzed_grants_defense_advantage() {
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &paralyzed), Some(true));
    }

    #[test]
    fn test_prone_grants_defense_advantage() {
        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &prone), Some(true));
    }

    #[test]
    fn test_blinded_grants_defense_advantage() {
        let blinded = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(2)),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &blinded), Some(true));
    }

    #[test]
    fn test_stunned_auto_fails_str_dex_saves() {
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(get_save_auto_fail(&stunned, Ability::Strength));
        assert!(get_save_auto_fail(&stunned, Ability::Dexterity));
        assert!(!get_save_auto_fail(&stunned, Ability::Constitution));
    }

    #[test]
    fn test_paralyzed_auto_fails_str_dex_saves() {
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        assert!(get_save_auto_fail(&paralyzed, Ability::Strength));
        assert!(get_save_auto_fail(&paralyzed, Ability::Dexterity));
        assert!(!get_save_auto_fail(&paralyzed, Ability::Wisdom));
    }

    #[test]
    fn test_stunned_incapacitated_cannot_act() {
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(!can_take_actions(&stunned));
        assert!(!can_take_reactions(&stunned));
    }

    #[test]
    fn test_paralyzed_incapacitated_cannot_act() {
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        assert!(!can_take_actions(&paralyzed));
        assert!(!can_take_reactions(&paralyzed));
    }

    #[test]
    fn test_poisoned_can_still_act() {
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert!(can_take_actions(&poisoned));
        assert!(can_take_reactions(&poisoned));
    }

    #[test]
    fn test_prone_reduces_speed() {
        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];
        assert_eq!(get_speed_multiplier(&prone), 0.5);
    }

    #[test]
    fn test_normal_speed_without_conditions() {
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_speed_multiplier(&empty), 1.0);
    }

    #[test]
    fn test_paralyzed_is_auto_crit_target() {
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        assert!(is_auto_crit_target(&paralyzed));
    }

    #[test]
    fn test_stunned_is_not_auto_crit_target() {
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(!is_auto_crit_target(&stunned));
    }

    #[test]
    fn test_tick_duration_decrements_rounds() {
        let mut condition = ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3));
        // Rounds(3) means active for rounds 1, 2, 3, then expires
        assert!(!tick_duration(&mut condition)); // 2 rounds remaining
        assert!(!tick_duration(&mut condition)); // 1 round remaining
        assert!(tick_duration(&mut condition));  // 0 rounds, expired
    }

    #[test]
    fn test_save_ends_does_not_tick() {
        let mut condition = ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::SaveEnds { ability: Ability::Constitution, dc: 10 },
        );
        assert!(!tick_duration(&mut condition));
        assert!(!tick_duration(&mut condition));
        // Still not expired
        match condition.duration {
            ConditionDuration::SaveEnds { .. } => (), // Still save ends
            _ => panic!("Should still be SaveEnds"),
        }
    }

    #[test]
    fn test_permanent_does_not_tick() {
        let mut condition = ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent);
        assert!(!tick_duration(&mut condition));
        assert!(!tick_duration(&mut condition));
    }

    #[test]
    fn test_active_condition_with_source() {
        let condition = ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::Rounds(2),
        ).with_source("Giant Spider");
        assert_eq!(condition.source, Some("Giant Spider".to_string()));
    }
}
