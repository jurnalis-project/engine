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

/// Get advantage/disadvantage on the attacker's roll based on conditions on the
/// defender (and, for Invisible, the attacker). Returns `Some(true)` for
/// advantage, `Some(false)` for disadvantage, `None` for no net effect.
///
/// Prone is handled here as "defaults to advantage"; the caller must downgrade
/// to disadvantage when the attacker is beyond 5 ft (see `combat/mod.rs`).
pub fn get_defense_advantage(
    _attacker_conditions: &[ActiveCondition],
    defender_conditions: &[ActiveCondition],
) -> Option<bool> {
    // Defenders that grant attackers advantage:
    let grants_advantage = has_condition(defender_conditions, ConditionType::Prone)
        || has_condition(defender_conditions, ConditionType::Stunned)
        || has_condition(defender_conditions, ConditionType::Paralyzed)
        || has_condition(defender_conditions, ConditionType::Petrified)
        || has_condition(defender_conditions, ConditionType::Restrained)
        || has_condition(defender_conditions, ConditionType::Unconscious)
        || has_condition(defender_conditions, ConditionType::Blinded);

    // Invisible defenders impose disadvantage on the attacker's rolls.
    let grants_disadvantage = has_condition(defender_conditions, ConditionType::Invisible);

    match (grants_advantage, grants_disadvantage) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        (true, true) => None, // advantage and disadvantage cancel
        (false, false) => None,
    }
}

/// Check if conditions cause auto-fail on a saving throw for the given ability.
///
/// Per SRD, Stunned, Paralyzed, Petrified, and Unconscious all auto-fail STR
/// and DEX saves. Other abilities are unaffected.
pub fn get_save_auto_fail(conditions: &[ActiveCondition], ability: Ability) -> bool {
    let triggers_auto_fail = has_condition(conditions, ConditionType::Stunned)
        || has_condition(conditions, ConditionType::Paralyzed)
        || has_condition(conditions, ConditionType::Petrified)
        || has_condition(conditions, ConditionType::Unconscious);

    triggers_auto_fail && matches!(ability, Ability::Strength | Ability::Dexterity)
}

/// Check if conditions impose disadvantage on a saving throw for the given ability.
///
/// Per SRD, Restrained imposes disadvantage on DEX saves. This is distinct from
/// auto-fail -- the roll still happens, just with disadvantage.
pub fn get_save_disadvantage(conditions: &[ActiveCondition], ability: Ability) -> bool {
    if has_condition(conditions, ConditionType::Restrained) && ability == Ability::Dexterity {
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

/// Get movement speed multiplier for conditions that scale speed.
///
/// Returns the multiplicative factor (Prone = 0.5). Callers should consult
/// `speed_is_zero` FIRST for hard zero-speed conditions (Grappled, Restrained,
/// Paralyzed, Petrified, Unconscious) -- those are not representable as a
/// multiplier.
pub fn get_speed_multiplier(conditions: &[ActiveCondition]) -> f32 {
    if has_condition(conditions, ConditionType::Prone) {
        0.5
    } else {
        1.0
    }
}

/// Check whether any condition forces the combatant's speed to 0.
///
/// Per SRD: Grappled, Restrained, Paralyzed, Petrified, Unconscious all
/// set speed to 0 (and prevent it from increasing).
pub fn speed_is_zero(conditions: &[ActiveCondition]) -> bool {
    has_condition(conditions, ConditionType::Grappled)
        || has_condition(conditions, ConditionType::Restrained)
        || has_condition(conditions, ConditionType::Paralyzed)
        || has_condition(conditions, ConditionType::Petrified)
        || has_condition(conditions, ConditionType::Unconscious)
}

/// Check if attacks against this target are automatic critical hits when the
/// attacker is within 5 ft. Callers must verify the distance themselves.
///
/// Per SRD: Paralyzed and Unconscious targets auto-crit from within 5 ft.
pub fn is_auto_crit_target(conditions: &[ActiveCondition]) -> bool {
    has_condition(conditions, ConditionType::Paralyzed)
        || has_condition(conditions, ConditionType::Unconscious)
}

/// Get initiative roll advantage/disadvantage from conditions.
///
/// - Invisible grants advantage on Initiative.
/// - Incapacitated (directly or derived) imposes disadvantage on Initiative.
/// - Both present cancel to no net effect.
pub fn get_initiative_advantage(conditions: &[ActiveCondition]) -> Option<bool> {
    let advantage = has_condition(conditions, ConditionType::Invisible);
    let disadvantage = is_incapacitated(conditions);

    match (advantage, disadvantage) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Check whether conditions grant resistance to all damage types. Currently
/// only Petrified does per SRD.
pub fn has_resistance_to_all(conditions: &[ActiveCondition]) -> bool {
    has_condition(conditions, ConditionType::Petrified)
}

/// Sensory channels used by ability checks. Callers pass the channel they
/// depend on so the query can report auto-fail when the relevant sense is
/// impaired by a condition (Blinded => sight, Deafened => hearing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenseChannel {
    Sight,
    Hearing,
}

/// Check if conditions cause auto-fail on an ability check that relies on a
/// specific sense. Per 2024 SRD:
/// - Blinded auto-fails any check that requires sight.
/// - Deafened auto-fails any check that requires hearing.
///
/// Callers decide whether their check depends on sight or hearing and pass
/// the corresponding `SenseChannel`.
pub fn get_ability_check_auto_fail(
    conditions: &[ActiveCondition],
    channel: SenseChannel,
) -> bool {
    match channel {
        SenseChannel::Sight => has_condition(conditions, ConditionType::Blinded),
        SenseChannel::Hearing => has_condition(conditions, ConditionType::Deafened),
    }
}

/// Check if conditions impose disadvantage on general ability checks (not
/// tied to a specific sense). Per 2024 SRD:
/// - Poisoned imposes disadvantage on all ability checks.
/// - Frightened imposes disadvantage on ability checks while the source of
///   the fear is in line of sight. Callers pass `source_visible` to indicate
///   this. Frightened with no visible source does not impose disadvantage.
/// - Exhaustion's D20 Tests penalty is numeric (see `exhaustion_d20_penalty`)
///   and layered separately by the caller.
pub fn get_ability_check_disadvantage(
    conditions: &[ActiveCondition],
    source_visible: bool,
) -> bool {
    if has_condition(conditions, ConditionType::Poisoned) {
        return true;
    }
    if source_visible && has_condition(conditions, ConditionType::Frightened) {
        return true;
    }
    false
}

/// Check if a Charmed attacker confers advantage on social ability checks
/// (Deception, Intimidation, Performance, Persuasion) made by the charmer
/// against the target. Per 2024 SRD, the charmer has advantage on such checks
/// while the target is charmed by them.
///
/// `source_name` identifies the would-be charmer; it is matched against the
/// `source` on the Charmed `ActiveCondition` (case-insensitive, trimmed).
pub fn charmer_has_social_advantage(
    target_conditions: &[ActiveCondition],
    source_name: &str,
) -> bool {
    let needle = source_name.trim().to_lowercase();
    for c in target_conditions {
        if c.condition == ConditionType::Charmed {
            if let Some(source) = c.source.as_deref() {
                if source.trim().to_lowercase() == needle {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if an attacker rolls with disadvantage against a specific target
/// because of Grappled. A Grappled creature rolls with disadvantage on attacks
/// against any target other than the grappler per 2024 SRD. The grappler is
/// identified by the `source` on the Grappled `ActiveCondition`.
///
/// Returns true if the attacker is Grappled AND the target is NOT the grappler.
pub fn grappled_attack_disadvantage(
    attacker_conditions: &[ActiveCondition],
    target_name: &str,
) -> bool {
    let needle = target_name.trim().to_lowercase();
    for c in attacker_conditions {
        if c.condition == ConditionType::Grappled {
            match c.source.as_deref() {
                Some(source) if source.trim().to_lowercase() == needle => return false,
                // Grappled with no source recorded: conservatively impose disadvantage
                // on any target (there is a grappler, we just don't know who).
                _ => return true,
            }
        }
    }
    false
}

/// Check if a Frightened creature can willingly move closer to the fear source.
/// Returns false if Frightened and the movement would reduce distance to the
/// source (caller supplies the source name and whether the intended move
/// reduces distance). Returns true otherwise.
pub fn can_move_closer_to(
    conditions: &[ActiveCondition],
    source_name: &str,
    move_reduces_distance: bool,
) -> bool {
    if !move_reduces_distance {
        return true;
    }
    let needle = source_name.trim().to_lowercase();
    for c in conditions {
        if c.condition == ConditionType::Frightened {
            if let Some(source) = c.source.as_deref() {
                if source.trim().to_lowercase() == needle {
                    return false;
                }
            } else {
                // Frightened without a known source: block any "closer" move
                // conservatively; the caller is responsible for recording
                // the source on apply.
                return false;
            }
        }
    }
    true
}

/// Damage-type / condition-type immunities conferred by a condition. Per 2024
/// SRD, Petrified grants immunity to the Poisoned condition and to poison/
/// disease damage types. This helper surfaces the condition-type immunity
/// used when applying new conditions.
pub fn is_immune_to_condition(
    conditions: &[ActiveCondition],
    incoming: ConditionType,
) -> bool {
    if has_condition(conditions, ConditionType::Petrified) && incoming == ConditionType::Poisoned {
        return true;
    }
    false
}

/// Apply a condition, honoring condition-type immunities. Returns true if the
/// condition was applied, false if it was rejected due to immunity. Callers
/// should use this instead of pushing directly onto the `conditions` vec.
pub fn apply_condition(
    conditions: &mut Vec<ActiveCondition>,
    new_condition: ActiveCondition,
) -> bool {
    if is_immune_to_condition(conditions, new_condition.condition) {
        return false;
    }
    conditions.push(new_condition);
    true
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

// ---- Exhaustion (2024 SRD unified formula) ----
//
// Exhaustion level is stored as `Character.exhaustion: u32`. It is not an
// `ActiveCondition` entry (single source of truth for the numeric level).
//
// Formula (level n in 0..=6):
//   D20 Tests: result -= 2 * n
//   Speed:     result -= 5 * n (feet; caller clamps at 0)
//   Death at n == 6.
//   Long rest: n -= 1 (saturating at 0; handled in rest module).

/// Penalty to apply to any D20 Test total given an exhaustion level.
/// Returns a negative i32 (or 0).
pub fn exhaustion_d20_penalty(level: u32) -> i32 {
    -2 * (level as i32)
}

/// Penalty (in feet) to apply to a creature's speed given an exhaustion level.
/// Returns a negative i32 (or 0). Callers are responsible for clamping final
/// speed at 0.
pub fn exhaustion_speed_penalty(level: u32) -> i32 {
    -5 * (level as i32)
}

/// Whether the given exhaustion level is lethal (>= 6 per SRD 2024).
pub fn exhaustion_is_lethal(level: u32) -> bool {
    level >= 6
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
    fn test_restrained_grants_defense_advantage() {
        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &restrained), Some(true));
    }

    #[test]
    fn test_petrified_grants_defense_advantage() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &petrified), Some(true));
    }

    #[test]
    fn test_unconscious_grants_defense_advantage() {
        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &unconscious), Some(true));
    }

    #[test]
    fn test_invisible_defender_grants_attacker_disadvantage() {
        let invisible = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(5)),
        ];
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_defense_advantage(&empty, &invisible), Some(false));
    }

    #[test]
    fn test_invisible_attacker_vs_visible_target_cancels_disadvantage_from_defender_blinded() {
        // Invisible attacker gains attack advantage already via get_attack_advantage.
        // Defender Blinded grants advantage too -- not stacked, still single advantage.
        let attacker = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(3)),
        ];
        let defender = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(3)),
        ];
        // Defense advantage path: invisible attacker does not change defender-side result.
        assert_eq!(get_defense_advantage(&attacker, &defender), Some(true));
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
    fn test_petrified_auto_fails_str_dex_saves() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        assert!(get_save_auto_fail(&petrified, Ability::Strength));
        assert!(get_save_auto_fail(&petrified, Ability::Dexterity));
        assert!(!get_save_auto_fail(&petrified, Ability::Charisma));
    }

    #[test]
    fn test_unconscious_auto_fails_str_dex_saves() {
        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        assert!(get_save_auto_fail(&unconscious, Ability::Strength));
        assert!(get_save_auto_fail(&unconscious, Ability::Dexterity));
        assert!(!get_save_auto_fail(&unconscious, Ability::Intelligence));
    }

    #[test]
    fn test_restrained_imposes_dex_save_disadvantage() {
        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        assert!(get_save_disadvantage(&restrained, Ability::Dexterity));
        assert!(!get_save_disadvantage(&restrained, Ability::Strength));
        // Restrained does NOT auto-fail DEX saves (only imposes disadvantage).
        assert!(!get_save_auto_fail(&restrained, Ability::Dexterity));
    }

    #[test]
    fn test_empty_conditions_no_save_modifiers() {
        let empty: Vec<ActiveCondition> = vec![];
        for ability in Ability::all() {
            assert!(!get_save_auto_fail(&empty, *ability));
            assert!(!get_save_disadvantage(&empty, *ability));
        }
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

    // --- Speed zero conditions ---

    #[test]
    fn test_grappled_sets_speed_zero() {
        let grappled = vec![
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent),
        ];
        assert!(speed_is_zero(&grappled));
    }

    #[test]
    fn test_restrained_sets_speed_zero() {
        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        assert!(speed_is_zero(&restrained));
    }

    #[test]
    fn test_paralyzed_sets_speed_zero() {
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(2)),
        ];
        assert!(speed_is_zero(&paralyzed));
    }

    #[test]
    fn test_petrified_sets_speed_zero() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        assert!(speed_is_zero(&petrified));
    }

    #[test]
    fn test_unconscious_sets_speed_zero() {
        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        assert!(speed_is_zero(&unconscious));
    }

    #[test]
    fn test_no_condition_speed_not_zero() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(!speed_is_zero(&empty));
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert!(!speed_is_zero(&poisoned));
    }

    // --- Auto-crit ---

    #[test]
    fn test_unconscious_is_auto_crit_target() {
        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        assert!(is_auto_crit_target(&unconscious));
    }

    // --- Initiative ---

    #[test]
    fn test_invisible_grants_initiative_advantage() {
        let invisible = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(5)),
        ];
        assert_eq!(get_initiative_advantage(&invisible), Some(true));
    }

    #[test]
    fn test_incapacitated_imposes_initiative_disadvantage() {
        let incap = vec![
            ActiveCondition::new(ConditionType::Incapacitated, ConditionDuration::Rounds(1)),
        ];
        assert_eq!(get_initiative_advantage(&incap), Some(false));
    }

    #[test]
    fn test_stunned_imposes_initiative_disadvantage_via_incap() {
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert_eq!(get_initiative_advantage(&stunned), Some(false));
    }

    #[test]
    fn test_invisible_plus_incap_cancels_initiative_mod() {
        let mixed = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(5)),
            ActiveCondition::new(ConditionType::Incapacitated, ConditionDuration::Rounds(1)),
        ];
        assert_eq!(get_initiative_advantage(&mixed), None);
    }

    #[test]
    fn test_no_initiative_modifier_without_conditions() {
        let empty: Vec<ActiveCondition> = vec![];
        assert_eq!(get_initiative_advantage(&empty), None);
    }

    // --- Damage resistance ---

    #[test]
    fn test_petrified_grants_resistance_to_all() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        assert!(has_resistance_to_all(&petrified));
    }

    #[test]
    fn test_other_conditions_no_blanket_resistance() {
        for condition in [
            ConditionType::Stunned,
            ConditionType::Paralyzed,
            ConditionType::Unconscious,
            ConditionType::Poisoned,
        ] {
            let conds = vec![ActiveCondition::new(condition, ConditionDuration::Rounds(1))];
            assert!(
                !has_resistance_to_all(&conds),
                "{:?} should NOT grant blanket resistance",
                condition
            );
        }
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

    // --- Exhaustion 2024 unified formula ---

    #[test]
    fn test_exhaustion_d20_penalty_scales_linearly() {
        assert_eq!(exhaustion_d20_penalty(0), 0);
        assert_eq!(exhaustion_d20_penalty(1), -2);
        assert_eq!(exhaustion_d20_penalty(3), -6);
        assert_eq!(exhaustion_d20_penalty(5), -10);
        assert_eq!(exhaustion_d20_penalty(6), -12);
    }

    #[test]
    fn test_exhaustion_speed_penalty_scales_linearly() {
        assert_eq!(exhaustion_speed_penalty(0), 0);
        assert_eq!(exhaustion_speed_penalty(1), -5);
        assert_eq!(exhaustion_speed_penalty(3), -15);
        assert_eq!(exhaustion_speed_penalty(5), -25);
        assert_eq!(exhaustion_speed_penalty(6), -30);
    }

    #[test]
    fn test_exhaustion_is_lethal_at_six() {
        assert!(!exhaustion_is_lethal(0));
        assert!(!exhaustion_is_lethal(5));
        assert!(exhaustion_is_lethal(6));
        // Defensive: treat any above-max value as lethal.
        assert!(exhaustion_is_lethal(99));
    }

    #[test]
    fn test_exhaustion_penalty_caps_arithmetically() {
        // Helper returns i32 so callers can just add it to a d20 total or speed.
        let roll: i32 = 15;
        let at_level_3 = roll + exhaustion_d20_penalty(3);
        assert_eq!(at_level_3, 9);

        let speed: i32 = 30;
        let adjusted = (speed + exhaustion_speed_penalty(4)).max(0);
        assert_eq!(adjusted, 10);

        // Speed floor at 0 if level would make it negative.
        let adjusted_zero = (speed + exhaustion_speed_penalty(6)).max(0);
        assert_eq!(adjusted_zero, 0);
    }

    #[test]
    fn test_active_condition_with_source() {
        let condition = ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::Rounds(2),
        ).with_source("Giant Spider");
        assert_eq!(condition.source, Some("Giant Spider".to_string()));
    }

    // --- Deafened: hearing-based ability check auto-fail ---

    #[test]
    fn test_deafened_auto_fails_hearing_checks() {
        let deafened = vec![
            ActiveCondition::new(ConditionType::Deafened, ConditionDuration::Rounds(3)),
        ];
        assert!(get_ability_check_auto_fail(&deafened, SenseChannel::Hearing));
        // Deafened does NOT auto-fail sight-based checks.
        assert!(!get_ability_check_auto_fail(&deafened, SenseChannel::Sight));
    }

    #[test]
    fn test_blinded_auto_fails_sight_checks() {
        let blinded = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(2)),
        ];
        assert!(get_ability_check_auto_fail(&blinded, SenseChannel::Sight));
        // Blinded does NOT auto-fail hearing checks.
        assert!(!get_ability_check_auto_fail(&blinded, SenseChannel::Hearing));
    }

    #[test]
    fn test_no_sense_auto_fail_without_relevant_condition() {
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert!(!get_ability_check_auto_fail(&poisoned, SenseChannel::Sight));
        assert!(!get_ability_check_auto_fail(&poisoned, SenseChannel::Hearing));
    }

    // --- Ability-check disadvantage (Poisoned, Frightened w/ visible source) ---

    #[test]
    fn test_poisoned_imposes_ability_check_disadvantage() {
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        // Poisoned applies regardless of source visibility.
        assert!(get_ability_check_disadvantage(&poisoned, false));
        assert!(get_ability_check_disadvantage(&poisoned, true));
    }

    #[test]
    fn test_frightened_ability_disadvantage_requires_visible_source() {
        let frightened = vec![
            ActiveCondition::new(ConditionType::Frightened, ConditionDuration::Rounds(3))
                .with_source("the demon"),
        ];
        // With source visible, disadvantage applies.
        assert!(get_ability_check_disadvantage(&frightened, true));
        // Without source visible, no disadvantage from Frightened alone.
        assert!(!get_ability_check_disadvantage(&frightened, false));
    }

    #[test]
    fn test_no_ability_check_disadvantage_without_trigger() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(!get_ability_check_disadvantage(&empty, true));
        assert!(!get_ability_check_disadvantage(&empty, false));
    }

    // --- Charmed: charmer gets advantage on social checks vs target ---

    #[test]
    fn test_charmer_has_social_advantage_vs_target() {
        let charmed_target = vec![
            ActiveCondition::new(ConditionType::Charmed, ConditionDuration::Rounds(5))
                .with_source("Hypnotist"),
        ];
        assert!(charmer_has_social_advantage(&charmed_target, "Hypnotist"));
        // Case-insensitive and trim-insensitive match.
        assert!(charmer_has_social_advantage(&charmed_target, "  hypnotist "));
        // Someone else trying a social check does NOT get advantage.
        assert!(!charmer_has_social_advantage(&charmed_target, "Goblin"));
    }

    #[test]
    fn test_non_charmed_target_grants_no_social_advantage() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(!charmer_has_social_advantage(&empty, "Hypnotist"));
    }

    // --- Grappled: disadvantage when attacking targets other than grappler ---

    #[test]
    fn test_grappled_imposes_disadvantage_vs_non_grappler() {
        let grappled = vec![
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("Ogre"),
        ];
        // Attacking a bystander => disadvantage.
        assert!(grappled_attack_disadvantage(&grappled, "Goblin"));
        // Attacking the grappler itself => no disadvantage from Grappled.
        assert!(!grappled_attack_disadvantage(&grappled, "Ogre"));
        // Case-insensitive match.
        assert!(!grappled_attack_disadvantage(&grappled, "ogre"));
    }

    #[test]
    fn test_grappled_without_source_imposes_disadvantage_on_all() {
        let grappled = vec![
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent),
        ];
        // Conservative: with no source recorded, treat all targets as non-grappler.
        assert!(grappled_attack_disadvantage(&grappled, "Anyone"));
    }

    #[test]
    fn test_not_grappled_no_disadvantage() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(!grappled_attack_disadvantage(&empty, "Anyone"));
    }

    // --- Frightened: can't move closer to source ---

    #[test]
    fn test_frightened_cannot_move_closer_to_source() {
        let frightened = vec![
            ActiveCondition::new(ConditionType::Frightened, ConditionDuration::Rounds(3))
                .with_source("Dragon"),
        ];
        // Move that reduces distance to source => blocked.
        assert!(!can_move_closer_to(&frightened, "Dragon", true));
        // Move away from source is still allowed.
        assert!(can_move_closer_to(&frightened, "Dragon", false));
        // Moving closer to a DIFFERENT source is allowed.
        assert!(can_move_closer_to(&frightened, "Goblin", true));
    }

    #[test]
    fn test_not_frightened_can_move_freely() {
        let empty: Vec<ActiveCondition> = vec![];
        assert!(can_move_closer_to(&empty, "Dragon", true));
        assert!(can_move_closer_to(&empty, "Dragon", false));
    }

    // --- Petrified: immunity to Poisoned condition ---

    #[test]
    fn test_petrified_is_immune_to_poisoned_condition() {
        let petrified = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        assert!(is_immune_to_condition(&petrified, ConditionType::Poisoned));
        // Petrified does NOT grant blanket condition immunity (only Poisoned).
        assert!(!is_immune_to_condition(&petrified, ConditionType::Blinded));
        assert!(!is_immune_to_condition(&petrified, ConditionType::Stunned));
    }

    #[test]
    fn test_apply_condition_respects_petrified_poison_immunity() {
        let mut conditions = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        let applied = apply_condition(
            &mut conditions,
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        );
        assert!(!applied, "Poisoned should be rejected for a Petrified target");
        assert!(!has_condition(&conditions, ConditionType::Poisoned));
    }

    #[test]
    fn test_apply_condition_allows_non_immune_conditions() {
        let mut conditions = vec![
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        ];
        let applied = apply_condition(
            &mut conditions,
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(2)),
        );
        assert!(applied);
        assert!(has_condition(&conditions, ConditionType::Blinded));
    }

    #[test]
    fn test_apply_condition_to_empty_target_works() {
        let mut conditions: Vec<ActiveCondition> = vec![];
        let applied = apply_condition(
            &mut conditions,
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        );
        assert!(applied);
        assert!(has_condition(&conditions, ConditionType::Poisoned));
    }
}
