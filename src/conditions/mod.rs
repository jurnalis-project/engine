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

/// Get attack roll advantage/disadvantage from conditions on the attacker.
///
/// Returns `Some(true)` for advantage, `Some(false)` for disadvantage, `None` for
/// no net effect. Per SRD, advantage and disadvantage from any source cancel to
/// neither.
///
/// Note: Stunned/Paralyzed/Petrified/Unconscious prevent attacks entirely via
/// incapacitation; that gating is enforced at the orchestrator level, not here.
pub fn get_attack_advantage(conditions: &[ActiveCondition]) -> Option<bool> {
    let has_disadvantage = has_condition(conditions, ConditionType::Poisoned)
        || has_condition(conditions, ConditionType::Blinded)
        || has_condition(conditions, ConditionType::Prone)
        || has_condition(conditions, ConditionType::Frightened)
        || has_condition(conditions, ConditionType::Restrained);

    let has_advantage = has_condition(conditions, ConditionType::Invisible);

    match (has_advantage, has_disadvantage) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None, // Both present cancel to none; neither present is none.
    }
}

/// Check whether an attacker can legally target a specific creature given their
/// conditions. Charmed attackers cannot target their charmer. Frightened
/// attackers can still attack their fear source (they just roll with disadvantage).
///
/// `source_name` on the active condition is matched against `target_name` for
/// charmer identification. Matching is case-insensitive and trims whitespace.
pub fn can_attack_target(
    attacker_conditions: &[ActiveCondition],
    target_name: &str,
) -> bool {
    let needle = target_name.trim().to_lowercase();
    for c in attacker_conditions {
        if c.condition == ConditionType::Charmed {
            if let Some(source) = c.source.as_deref() {
                if source.trim().to_lowercase() == needle {
                    return false;
                }
            }
        }
    }
    true
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

/// Check if a combatant is incapacitated (directly or via a derived condition).
///
/// Stunned, Paralyzed, Petrified, and Unconscious all include the Incapacitated
/// condition per SRD. This helper centralizes that derivation so callers don't
/// have to enumerate every trigger.
pub fn is_incapacitated(conditions: &[ActiveCondition]) -> bool {
    has_condition(conditions, ConditionType::Incapacitated)
        || has_condition(conditions, ConditionType::Stunned)
        || has_condition(conditions, ConditionType::Paralyzed)
        || has_condition(conditions, ConditionType::Petrified)
        || has_condition(conditions, ConditionType::Unconscious)
}

/// Check if the combatant can take actions.
pub fn can_take_actions(conditions: &[ActiveCondition]) -> bool {
    !is_incapacitated(conditions)
}

/// Check if the combatant can take reactions.
pub fn can_take_reactions(conditions: &[ActiveCondition]) -> bool {
    !is_incapacitated(conditions)
}

/// Check if the combatant can take bonus actions.
pub fn can_take_bonus_actions(conditions: &[ActiveCondition]) -> bool {
    !is_incapacitated(conditions)
}

/// Check if the combatant can speak. Incapacitated blocks speech per SRD.
pub fn can_speak(conditions: &[ActiveCondition]) -> bool {
    !is_incapacitated(conditions)
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
    fn test_frightened_imposes_attack_disadvantage() {
        let frightened = vec![
            ActiveCondition::new(ConditionType::Frightened, ConditionDuration::Rounds(2)),
        ];
        assert_eq!(get_attack_advantage(&frightened), Some(false));
    }

    #[test]
    fn test_restrained_imposes_attack_disadvantage() {
        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        assert_eq!(get_attack_advantage(&restrained), Some(false));
    }

    #[test]
    fn test_invisible_grants_attack_advantage() {
        let invisible = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(5)),
        ];
        assert_eq!(get_attack_advantage(&invisible), Some(true));
    }

    // --- Charmed targeting ---

    #[test]
    fn test_charmed_cannot_target_charmer() {
        let charmed = vec![
            ActiveCondition::new(ConditionType::Charmed, ConditionDuration::Rounds(5))
                .with_source("Hypnotist"),
        ];
        assert!(!can_attack_target(&charmed, "Hypnotist"));
        // Case-insensitive match.
        assert!(!can_attack_target(&charmed, "hypnotist"));
        // Other targets are still legal.
        assert!(can_attack_target(&charmed, "Goblin"));
    }

    #[test]
    fn test_charmed_without_source_restricts_nobody() {
        // Degenerate: Charmed with no source recorded can attack anyone.
        // Caller is responsible for recording the source when applying.
        let charmed = vec![
            ActiveCondition::new(ConditionType::Charmed, ConditionDuration::Rounds(5)),
        ];
        assert!(can_attack_target(&charmed, "Hypnotist"));
    }

    #[test]
    fn test_no_charmed_means_no_restriction() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(can_attack_target(&empty, "Anyone"));
    }

    #[test]
    fn test_advantage_and_disadvantage_cancel_returns_none() {
        // Invisible grants advantage, Poisoned imposes disadvantage -- cancel.
        let mixed = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(3)),
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        ];
        assert_eq!(get_attack_advantage(&mixed), None);
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

    // --- Incapacitated stacking ---

    #[test]
    fn test_is_incapacitated_detects_direct_condition() {
        let incap = vec![
            ActiveCondition::new(ConditionType::Incapacitated, ConditionDuration::Rounds(1)),
        ];
        assert!(is_incapacitated(&incap));
    }

    #[test]
    fn test_is_incapacitated_detects_derived_conditions() {
        // Stunned, Paralyzed, Petrified, Unconscious all imply Incapacitated.
        for condition in [
            ConditionType::Stunned,
            ConditionType::Paralyzed,
            ConditionType::Petrified,
            ConditionType::Unconscious,
        ] {
            let conds = vec![ActiveCondition::new(condition, ConditionDuration::Rounds(1))];
            assert!(
                is_incapacitated(&conds),
                "{:?} should imply incapacitated",
                condition
            );
        }
    }

    #[test]
    fn test_is_incapacitated_false_without_trigger() {
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert!(!is_incapacitated(&poisoned));
        let empty: Vec<ActiveCondition> = vec![];
        assert!(!is_incapacitated(&empty));
    }

    #[test]
    fn test_incapacitated_cannot_take_bonus_actions_or_speak() {
        let incap = vec![
            ActiveCondition::new(ConditionType::Incapacitated, ConditionDuration::Rounds(1)),
        ];
        assert!(!can_take_actions(&incap));
        assert!(!can_take_reactions(&incap));
        assert!(!can_take_bonus_actions(&incap));
        assert!(!can_speak(&incap));
    }

    #[test]
    fn test_unconscious_cannot_act() {
        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        assert!(!can_take_actions(&unconscious));
        assert!(!can_take_reactions(&unconscious));
        assert!(!can_take_bonus_actions(&unconscious));
    }

    #[test]
    fn test_petrified_cannot_act() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        assert!(!can_take_actions(&petrified));
        assert!(!can_take_bonus_actions(&petrified));
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
