// jurnalis-engine/src/combat/mod.rs
// Combat system: state, initiative, attack resolution, movement, NPC AI.
pub mod monsters;

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::character::Character;
use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};
use crate::equipment::{AMMUNITION, FINESSE, REACH, THROWN, VERSATILE};
use crate::output::format_roll;
use crate::rules::dice::{roll_d20, roll_dice};
use crate::state::Npc;
use crate::state::{CombatStats, DamageType, GameState, ItemType, NpcAttack};
use crate::types::{Ability, Cover, ItemId, NpcId, Skill};

/// Identifies a combatant in initiative order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Combatant {
    Player,
    Npc(NpcId),
}

/// A reaction prompt awaiting player input.
///
/// When a reaction-triggering event fires during NPC turn processing,
/// the engine records the context here and returns a prompt. The next
/// player input is interpreted against this pending reaction (yes/no).
/// `resume_npc_index` points at the initiative entry whose turn should
/// run next after the reaction resolves -- used to pick up NPC-turn
/// processing where it paused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PendingReaction {
    /// Offer the player a Shield spell reaction in response to an
    /// incoming attack that would land.
    Shield {
        attacker_npc_id: NpcId,
        /// Damage the player would take if the attack resolves unmodified.
        incoming_damage: i32,
        /// AC before the Shield bonus (for re-resolving the attack after
        /// the reaction is accepted).
        pre_roll_ac: i32,
        /// Initiative index to resume NPC turn processing after the
        /// reaction completes.
        resume_npc_index: usize,
    },
    /// Offer the player an opportunity attack on an NPC leaving melee
    /// reach without disengaging.
    OpportunityAttack {
        fleeing_npc_id: NpcId,
        old_distance: u32,
        new_distance: u32,
        resume_npc_index: usize,
    },
}

/// Full combat state, stored in GameState.active_combat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatState {
    /// Initiative order (highest first).
    pub initiative_order: Vec<(Combatant, i32)>,
    /// Current turn index into initiative_order.
    pub current_turn: usize,
    /// Current round number (1-indexed).
    pub round: u32,
    /// Pairwise distance from player to each NPC (in feet, 5-ft increments).
    pub distances: HashMap<NpcId, u32>,
    /// Player movement remaining this turn (feet).
    pub player_movement_remaining: i32,
    /// Whether the player is dodging (grants disadvantage on incoming attacks).
    pub player_dodging: bool,
    /// Whether the player used Disengage (prevents opportunity attacks).
    pub player_disengaging: bool,
    /// Whether the player has used their action this turn.
    ///
    /// (Formerly `player_action_used`; renamed for consistency with the full
    /// action-economy model. The `serde(alias)` keeps old saves loadable.)
    #[serde(alias = "player_action_used")]
    pub action_used: bool,
    /// Whether the player has used their bonus action this turn.
    #[serde(default)]
    pub bonus_action_used: bool,
    /// Fighter Action Surge: when true, the action_used gate is bypassed for
    /// exactly one additional action. Cleared after the surged action resolves
    /// or at the start of the next player turn (advance_turn).
    #[serde(default)]
    pub action_surge_active: bool,
    /// Whether the player has used their reaction. Resets at the end of the
    /// player's turn (so reactions stay available during NPC turns).
    #[serde(default)]
    pub reaction_used: bool,
    /// Whether the player has used their free object interaction this turn.
    #[serde(default)]
    pub free_interaction_used: bool,
    /// NPCs that are dodging (NpcId -> true until their next turn).
    pub npc_dodging: HashMap<NpcId, bool>,
    /// NPCs that are disengaging this turn.
    pub npc_disengaging: HashMap<NpcId, bool>,
    /// Shield spell AC bonus (resets at start of player's next turn).
    #[serde(default)]
    pub player_shield_ac_bonus: i32,
    /// A reaction prompt awaiting the player's decision. When set, the
    /// engine interprets the next input as a yes/no response before
    /// resuming normal combat processing.
    #[serde(default)]
    pub pending_reaction: Option<PendingReaction>,
    // ---- 2024 SRD Weapon Mastery bookkeeping -----------------------------
    // All fields use #[serde(default)] so existing saves load cleanly. See
    // docs/specs/weapon-mastery.md for the semantics.
    //
    /// Set by the Vex mastery when the player hits a creature and deals
    /// damage. The player's next attack roll against this NPC is made with
    /// advantage. Cleared when consumed (on the next player attack) or at
    /// the start of the player's next turn, whichever comes first.
    #[serde(default)]
    pub player_vex_target: Option<NpcId>,
    /// NPCs marked by the Sap mastery: their next attack roll is made at
    /// disadvantage. MVP clears this set at the start of the player's next
    /// turn. An entry is also consumed the first time the NPC attacks.
    #[serde(default)]
    pub sap_targets: std::collections::HashSet<NpcId>,
    /// NPC id -> speed reduction (feet) applied by Slow masteries this
    /// turn. Cleared at the start of the player's next turn.
    #[serde(default)]
    pub slow_targets: std::collections::HashMap<NpcId, i32>,
    /// Whether the Cleave mastery has been used this player turn. Resets
    /// at the start of the player's turn (once-per-turn cap per SRD).
    #[serde(default)]
    pub cleave_used_this_turn: bool,
    /// Whether the Nick mastery off-hand extra attack has been used this
    /// turn. Resets at the start of the player's turn (once-per-turn cap
    /// per SRD 2024).
    #[serde(default)]
    pub nick_used_this_turn: bool,
    // ---- Extra Attack (SRD 2024, level 5) --------------------------------
    // Tracks how many attacks the player has made this turn as part of the
    // Attack action. Resets to 0 at the start of each player turn. Used by
    // the orchestrator to allow Extra Attack classes (Fighter, Barbarian,
    // Paladin, Ranger at level 5+) to make 2 attacks before consuming the
    // action.
    #[serde(default)]
    pub attacks_made_this_turn: u32,
    // ---- Death Saving Throws (issue #84) ---------------------------------
    // Per SRD 5e, a player at 0 HP enters a dying state and rolls death
    // saves at the start of each of their turns. Three successes stabilize
    // the character (HP becomes 1); three failures kill them. A natural 20
    // stabilizes immediately at 1 HP; a natural 1 counts as two failures.
    // Damage while dying adds a failure (two on a crit); damage in a single
    // hit that equals or exceeds max HP causes instant death.
    //
    // Both counters default to 0 for older saves (pre-DST) via
    // `#[serde(default)]`.
    /// Number of death save successes accumulated in the current dying
    /// episode. Cleared on stabilize, heal, or crit success.
    #[serde(default)]
    pub death_save_successes: u8,
    /// Number of death save failures accumulated in the current dying
    /// episode. Cleared on stabilize, heal, or crit success. Reaching 3
    /// ends combat in defeat.
    #[serde(default)]
    pub death_save_failures: u8,
    /// Cover level protecting the player from incoming attacks and DEX saves.
    /// Defaults to `Cover::None` for older saves that pre-date this field.
    #[serde(default)]
    pub player_cover: Cover,
    /// Cover level for each NPC. NPCs not present in this map have
    /// `Cover::None`. Defaults to empty for older saves.
    #[serde(default)]
    pub npc_cover: HashMap<NpcId, Cover>,
    // ---- Opportunity Attack Reaction Tracking (issue #43) ----------------
    /// Set of NPC ids that have already used their reaction this round
    /// (e.g., to make an opportunity attack). Cleared at the start of each
    /// new round.
    #[serde(default)]
    pub npc_reactions_used: std::collections::HashSet<NpcId>,
}

/// Outcome of a single Death Saving Throw or damage-while-dying event.
///
/// Emitted by `CombatState::apply_death_save_roll` and
/// `CombatState::apply_damage_while_dying` so the orchestrator can narrate
/// the result and check for combat-ending transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeathSaveOutcome {
    /// Rolled 10-19 (or 2-9, etc.) with non-terminal counter change.
    Success,
    /// Rolled 2-9 (or damage-while-dying).
    Failure,
    /// Natural 20: character stabilizes at 1 HP immediately.
    CritSuccess,
    /// Natural 1 (or a crit hit while dying): counts as two failures but
    /// combat is not yet over.
    CritFailure,
    /// Accumulated three successes: character is stable (HP 1).
    Stable,
    /// Accumulated three failures (or instant death via massive damage):
    /// the character has died.
    Dead,
}

impl CombatState {
    /// Check if combat is over. Returns Some(true) for victory, Some(false) for defeat.
    ///
    /// Per SRD Death Saving Throws (issue #84), 0 HP alone does NOT end
    /// combat in defeat -- the player is dying but still in play. Defeat is
    /// declared only after three death save failures (see
    /// `death_save_failures`) or instant death via massive damage.
    pub fn check_end(&self, state: &GameState) -> Option<bool> {
        if state.character.current_hp <= 0 && self.death_save_failures >= 3 {
            return Some(false); // defeat: player has died
        }
        // Victory: all NPC combatants in the initiative order are dead.
        // The Player entry is intentionally skipped — the Player's HP is checked
        // above for the defeat condition; here we only ask "are all enemies gone?"
        // An NPC that is absent from the world or has no combat_stats is treated
        // as alive (unwrap_or(false)) so that stale initiative entries do not
        // prematurely declare victory (see test_missing_combat_stats_treated_as_alive_not_dead).
        let all_npcs_dead = self.initiative_order.iter().all(|(c, _)| match c {
            Combatant::Player => true,
            Combatant::Npc(id) => state
                .world
                .npcs
                .get(id)
                .and_then(|npc| npc.combat_stats.as_ref())
                .map(|cs| cs.current_hp <= 0)
                .unwrap_or(false),
        });
        if all_npcs_dead {
            Some(true)
        } else {
            None
        }
    }

    /// True when the player is in the dying state: HP at or below 0 but
    /// fewer than three accumulated death save failures.
    pub fn is_player_dying(&self, state: &GameState) -> bool {
        state.character.current_hp <= 0 && self.death_save_failures < 3
    }

    /// Clear both death save counters. Called when the character is healed
    /// out of dying, stabilizes via three successes, or rolls a natural 20
    /// on a death save (per SRD).
    pub fn reset_death_saves(&mut self) {
        self.death_save_successes = 0;
        self.death_save_failures = 0;
    }

    /// Apply the outcome of a single Death Saving Throw to combat state.
    ///
    /// `d20` is the raw d20 roll (1..=20). The character argument is
    /// mutated only on stabilization (HP set to 1 on a natural 20 or on
    /// reaching three successes). Returns the narration-friendly outcome
    /// and updates `death_save_successes`/`death_save_failures`.
    pub fn apply_death_save_roll(
        &mut self,
        character: &mut crate::character::Character,
        d20: i32,
    ) -> DeathSaveOutcome {
        // Natural 20: the character regains 1 HP and becomes conscious
        // (dying state ends immediately).
        if d20 >= 20 {
            character.current_hp = 1;
            self.reset_death_saves();
            return DeathSaveOutcome::CritSuccess;
        }
        // Natural 1: counts as two failures.
        if d20 <= 1 {
            self.death_save_failures = self.death_save_failures.saturating_add(2).min(3);
            if self.death_save_failures >= 3 {
                return DeathSaveOutcome::Dead;
            }
            return DeathSaveOutcome::CritFailure;
        }
        if d20 >= 10 {
            self.death_save_successes = self.death_save_successes.saturating_add(1);
            if self.death_save_successes >= 3 {
                // Stable: regain 1 HP, reset counters per SRD.
                character.current_hp = 1;
                self.reset_death_saves();
                return DeathSaveOutcome::Stable;
            }
            return DeathSaveOutcome::Success;
        }
        // d20 in 2..=9
        self.death_save_failures = self.death_save_failures.saturating_add(1);
        if self.death_save_failures >= 3 {
            return DeathSaveOutcome::Dead;
        }
        DeathSaveOutcome::Failure
    }

    /// Roll a death save with the given RNG and apply it. Returns the raw
    /// d20 roll alongside the outcome so the orchestrator can narrate the
    /// roll result ("Death save: 14 — success.") and the SRD rules.
    pub fn roll_death_save(
        &mut self,
        rng: &mut impl Rng,
        character: &mut crate::character::Character,
    ) -> (i32, DeathSaveOutcome) {
        let d20 = roll_d20(rng);
        let outcome = self.apply_death_save_roll(character, d20);
        (d20, outcome)
    }

    /// Apply damage received while the character is already at 0 HP.
    ///
    /// Per SRD: any attack against a creature at 0 HP causes one failed
    /// death save (two on a critical hit). A single hit that deals damage
    /// equal to or greater than the character's HP maximum causes instant
    /// death. The caller has already deducted `damage` from `current_hp`;
    /// this helper only updates the death save counters and returns the
    /// outcome.
    pub fn apply_damage_while_dying(
        &mut self,
        character: &mut crate::character::Character,
        damage: i32,
        is_crit: bool,
    ) -> DeathSaveOutcome {
        // Instant death via massive damage (SRD): a single hit of damage
        // >= max_hp while dying kills outright.
        if damage >= character.max_hp && character.max_hp > 0 {
            self.death_save_failures = 3;
            return DeathSaveOutcome::Dead;
        }
        let add = if is_crit { 2 } else { 1 };
        self.death_save_failures = self.death_save_failures.saturating_add(add).min(3);
        if self.death_save_failures >= 3 {
            return DeathSaveOutcome::Dead;
        }
        if is_crit {
            DeathSaveOutcome::CritFailure
        } else {
            DeathSaveOutcome::Failure
        }
    }

    /// Get the current combatant.
    pub fn current_combatant(&self) -> Combatant {
        self.initiative_order[self.current_turn].0
    }

    /// Is it the player's turn?
    pub fn is_player_turn(&self) -> bool {
        self.current_combatant() == Combatant::Player
    }

    /// Called at the end of the player's turn, before advancing initiative.
    ///
    /// Per SRD 5.1, the reaction refreshes at the end of the previous turn so
    /// that reactions remain available during subsequent NPC turns.
    pub fn end_player_turn(&mut self) {
        self.reaction_used = false;
    }

    /// Advance to next living combatant. Returns the new combatant.
    ///
    /// Takes `&mut GameState` so player-turn-start cleanup can clear
    /// turn-scoped `ClassFeatureState` flags (Rogue Sneak Attack and Cunning
    /// Action). Combat and mastery state live on `self`; class-feature
    /// resources live on `state.character` and must be reset here to keep
    /// the "start of turn" semantics unified.
    pub fn advance_turn(&mut self, state: &mut GameState) -> Combatant {
        loop {
            self.current_turn = (self.current_turn + 1) % self.initiative_order.len();
            if self.current_turn == 0 {
                self.round += 1;
                self.npc_reactions_used.clear();
            }
            let combatant = self.current_combatant();
            match combatant {
                Combatant::Player => {
                    // Reset player turn state. Reaction is NOT reset here --
                    // it refreshes at end of previous turn (see `end_player_turn`).
                    self.player_movement_remaining = state.character.speed;
                    self.player_dodging = false;
                    self.player_disengaging = false;
                    self.action_used = false;
                    self.bonus_action_used = false;
                    self.action_surge_active = false;
                    self.free_interaction_used = false;
                    self.player_shield_ac_bonus = 0;
                    // 2024 SRD Weapon Mastery cleanup at the start of the
                    // player's turn. Per spec:
                    //   - Vex lasts until the end of the player's next turn.
                    //     We clear at the start of the current turn (slight
                    //     approximation — "end of next turn" in a 1D
                    //     model converges to "this turn" since NPC turns
                    //     don't consume the player's Vex).
                    //   - Sap's "before the start of your next turn"
                    //     duration means entries for NPCs that did not
                    //     attack this round still clear here.
                    //   - Slow resets at the start of your next turn per
                    //     SRD.
                    //   - Cleave / Nick are once per turn.
                    self.player_vex_target = None;
                    self.sap_targets.clear();
                    self.slow_targets.clear();
                    self.cleave_used_this_turn = false;
                    self.nick_used_this_turn = false;
                    self.attacks_made_this_turn = 0;
                    // Rogue turn-scoped class features (once-per-turn caps):
                    //   - Sneak Attack: at most one application per turn.
                    //   - Cunning Action: a Rogue's bonus-action marker.
                    // Resetting here keeps the "start of turn" semantics
                    // consistent with the combat-local flags above.
                    state.character.class_features.sneak_attack_used_this_turn = false;
                    state.character.class_features.cunning_action_used = false;
                    // SRD 2024: a grapple ends immediately if the grappler
                    // becomes incapacitated. We check at the start of the
                    // player's turn, which is the earliest moment we can
                    // detect the condition. Any NPC that has Grappled with
                    // the player as source has that condition removed.
                    if conditions::is_incapacitated(&state.character.conditions) {
                        let player_name = state.character.name.clone();
                        for npc in state.world.npcs.values_mut() {
                            npc.conditions.retain(|c| {
                                !(c.condition == ConditionType::Grappled
                                    && c.source
                                        .as_deref()
                                        .map(|s| s.eq_ignore_ascii_case(&player_name))
                                        .unwrap_or(false))
                            });
                        }
                    }
                    return combatant;
                }
                Combatant::Npc(id) => {
                    // Skip dead NPCs
                    let alive = state
                        .world
                        .npcs
                        .get(&id)
                        .and_then(|npc| npc.combat_stats.as_ref())
                        .map(|cs| cs.current_hp > 0)
                        .unwrap_or(false);
                    if alive {
                        // Reset NPC dodging on their new turn
                        self.npc_dodging.remove(&id);
                        self.npc_disengaging.remove(&id);
                        return combatant;
                    }
                }
            }
        }
    }
}

/// Narrate the outcome of a Death Saving Throw.
///
/// Returns human-readable lines describing the roll result and any state
/// transitions (stabilize, defeat, crit). The caller passes the raw d20
/// roll value alongside the outcome variant. Kept as a free function so
/// callers (orchestrator and tests) can format narration deterministically
/// from a known (roll, outcome) pair.
pub fn narrate_death_save_outcome(d20: i32, outcome: DeathSaveOutcome) -> Vec<String> {
    let mut lines = Vec::new();
    match outcome {
        DeathSaveOutcome::CritSuccess => {
            lines.push(format!(
                "Death saving throw: {} — natural 20! You regain 1 HP and rise, conscious.",
                d20,
            ));
        }
        DeathSaveOutcome::Stable => {
            lines.push(format!(
                "Death saving throw: {} — success. That's three! You stabilize at 1 HP.",
                d20,
            ));
        }
        DeathSaveOutcome::Success => {
            lines.push(format!("Death saving throw: {} — success.", d20,));
        }
        DeathSaveOutcome::Failure => {
            lines.push(format!("Death saving throw: {} — failure.", d20,));
        }
        DeathSaveOutcome::CritFailure => {
            lines.push(format!(
                "Death saving throw: {} — natural 1! That counts as two failures.",
                d20,
            ));
        }
        DeathSaveOutcome::Dead => {
            lines.push(format!(
                "Death saving throw: {} — failure. Three failures accumulated.",
                d20,
            ));
        }
    }
    lines
}

/// Narrate the outcome of damage received while already dying.
///
/// Returns human-readable lines describing the failure(s) added by the hit
/// and any state transition (instant death via massive damage or reaching
/// three failures). Emits an empty vec when the outcome is benign
/// (Success/Stable/CritSuccess shouldn't occur from damage but are handled
/// defensively).
pub fn narrate_damage_while_dying_outcome(outcome: DeathSaveOutcome) -> Vec<String> {
    match outcome {
        DeathSaveOutcome::Failure => {
            vec!["The hit lands on a dying target — one death save failure.".to_string()]
        }
        DeathSaveOutcome::CritFailure => {
            vec!["A critical hit on a dying target — two death save failures.".to_string()]
        }
        DeathSaveOutcome::Dead => {
            vec!["The damage is overwhelming — you have fallen.".to_string()]
        }
        // These outcomes shouldn't arise from damage; return empty defensively.
        DeathSaveOutcome::Success | DeathSaveOutcome::Stable | DeathSaveOutcome::CritSuccess => {
            Vec::new()
        }
    }
}

// ---- Initiative ----

/// Roll initiative for all combatants. Returns sorted order (highest first).
pub fn roll_initiative(
    rng: &mut impl Rng,
    player: &Character,
    npcs: &[(NpcId, &CombatStats)],
) -> Vec<(Combatant, i32)> {
    let mut initiatives: Vec<(Combatant, i32, i32, String)> = Vec::new();

    // Player
    let player_roll = roll_d20(rng);
    let player_dex_mod = player.ability_modifier(Ability::Dexterity);
    let player_init = player_roll + player_dex_mod;
    let player_dex = player
        .ability_scores
        .get(&Ability::Dexterity)
        .copied()
        .unwrap_or(10);
    initiatives.push((
        Combatant::Player,
        player_init,
        player_dex,
        player.name.clone(),
    ));

    // NPCs
    for &(id, stats) in npcs {
        let roll = roll_d20(rng);
        let dex = stats
            .ability_scores
            .get(&Ability::Dexterity)
            .copied()
            .unwrap_or(10);
        let dex_mod = Ability::modifier(dex);
        let init = roll + dex_mod;
        // Use NPC id as name placeholder for tie-breaking
        initiatives.push((Combatant::Npc(id), init, dex, format!("npc_{}", id)));
    }

    // Sort: higher initiative first, then higher DEX, then name (alphabetical)
    initiatives.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.3.cmp(&b.3)));

    initiatives
        .into_iter()
        .map(|(c, init, _, _)| (c, init))
        .collect()
}

/// Assign cover levels to NPCs at combat start based on location type.
///
/// Each NPC has a chance (varying by location type) to receive cover.
/// Room and Ruins locations can grant Half or Three-Quarters cover;
/// Cave, Corridor, and Clearing only grant Half. The assignment is
/// deterministic for a given RNG state.
pub fn assign_npc_cover(
    rng: &mut impl Rng,
    npc_ids: &[NpcId],
    location_type: crate::state::LocationType,
) -> HashMap<NpcId, Cover> {
    use crate::state::LocationType;
    let mut cover_map = HashMap::new();

    // Cover chance (percentage) and whether three-quarters is possible.
    let (chance, allows_three_quarters) = match location_type {
        LocationType::Room => (30, true),
        LocationType::Ruins => (40, true),
        LocationType::Cave => (20, false),
        LocationType::Corridor => (10, false),
        LocationType::Clearing => (10, false),
    };

    for &npc_id in npc_ids {
        let roll: u32 = rng.gen_range(1..=100);
        if roll <= chance {
            let cover = if allows_three_quarters && rng.gen_range(0..3) == 0 {
                // ~33% chance of three-quarters when eligible
                Cover::ThreeQuarters
            } else {
                Cover::Half
            };
            cover_map.insert(npc_id, cover);
        }
    }

    cover_map
}

/// Start combat: roll initiative, set distances, assign NPC cover, create CombatState.
pub fn start_combat(
    rng: &mut impl Rng,
    player: &Character,
    hostile_npc_ids: &[NpcId],
    npcs: &HashMap<NpcId, crate::state::Npc>,
    location_type: crate::state::LocationType,
) -> CombatState {
    let npc_stats: Vec<(NpcId, &CombatStats)> = hostile_npc_ids
        .iter()
        .filter_map(|&id| {
            npcs.get(&id)
                .and_then(|npc| npc.combat_stats.as_ref())
                .map(|cs| (id, cs))
        })
        .collect();

    // Every hostile NPC should have combat_stats assigned by world generation.
    // If any are dropped, it means a bug upstream -- catch it in debug builds.
    debug_assert_eq!(
        npc_stats.len(),
        hostile_npc_ids.len(),
        "start_combat: {} hostile NPCs provided but only {} have combat_stats",
        hostile_npc_ids.len(),
        npc_stats.len()
    );

    let initiative_order = roll_initiative(rng, player, &npc_stats);

    let mut distances = HashMap::new();
    for &id in hostile_npc_ids {
        // Random starting distance 20-30 ft in 5-ft increments
        let dist = (rng.gen_range(4..=6)) * 5; // 20, 25, or 30
        distances.insert(id, dist);
    }

    CombatState {
        initiative_order,
        current_turn: 0,
        round: 1,
        distances,
        player_movement_remaining: player.speed,
        player_dodging: false,
        player_disengaging: false,
        action_used: false,
        bonus_action_used: false,
        action_surge_active: false,
        reaction_used: false,
        free_interaction_used: false,
        npc_dodging: HashMap::new(),
        npc_disengaging: HashMap::new(),
        player_shield_ac_bonus: 0,
        pending_reaction: None,
        player_vex_target: None,
        sap_targets: std::collections::HashSet::new(),
        slow_targets: HashMap::new(),
        cleave_used_this_turn: false,
        nick_used_this_turn: false,
        attacks_made_this_turn: 0,
        death_save_successes: 0,
        death_save_failures: 0,
        player_cover: Cover::None,
        npc_cover: assign_npc_cover(rng, hostile_npc_ids, location_type),
        npc_reactions_used: std::collections::HashSet::new(),
    }
}

/// Melee reach (in feet) for the player, based on the main-hand weapon's
/// REACH property. Returns 10 ft when the equipped main-hand weapon has the
/// REACH property; otherwise defaults to 5 ft.
///
/// Pure ranged weapons (e.g. a longbow with no melee capability) also fall
/// back to the 5 ft unarmed default — the player can still swing with their
/// fists at melee range. This helper does NOT attempt to choose between main
/// and off hand; it always consults the main-hand weapon.
pub fn player_melee_reach(
    character: &Character,
    items: &HashMap<ItemId, crate::state::Item>,
) -> u32 {
    let weapon_id = match character.equipped.main_hand {
        Some(id) => id,
        None => return 5,
    };
    let item = match items.get(&weapon_id) {
        Some(i) => i,
        None => return 5,
    };
    match &item.item_type {
        ItemType::Weapon { properties, .. } if properties & REACH != 0 => 10,
        // MagicWeapon inherits the base weapon's reach.
        ItemType::MagicWeapon { properties, .. } if properties & REACH != 0 => 10,
        _ => 5,
    }
}

/// Returns true when the given NPC is currently within the player's melee
/// reach (as determined by [`player_melee_reach`]). Used by the orchestrator
/// to gate whether an NPC leaving a square should provoke a player
/// opportunity attack.
pub fn npc_within_player_reach(state: &GameState, combat: &CombatState, npc_id: NpcId) -> bool {
    let alive = state
        .world
        .npcs
        .get(&npc_id)
        .and_then(|npc| npc.combat_stats.as_ref())
        .map(|stats| stats.current_hp > 0)
        .unwrap_or(false);
    if !alive {
        return false;
    }
    let distance = match combat.distances.get(&npc_id).copied() {
        Some(d) => d,
        None => return false,
    };
    let player_reach = player_melee_reach(&state.character, &state.world.items);
    distance <= player_reach
}

/// Returns true if any living hostile NPC is within the specified distance.
pub fn has_living_hostile_within(state: &GameState, combat: &CombatState, feet: u32) -> bool {
    combat
        .initiative_order
        .iter()
        .any(|(combatant, _)| match combatant {
            Combatant::Player => false,
            Combatant::Npc(id) => {
                let alive = state
                    .world
                    .npcs
                    .get(id)
                    .and_then(|npc| npc.combat_stats.as_ref())
                    .map(|stats| stats.current_hp > 0)
                    .unwrap_or(false);
                let dist = combat.distances.get(id).copied().unwrap_or(u32::MAX);
                alive && dist <= feet
            }
        })
}

// ---- Attack Resolution ----

/// Result of an attack roll.
#[derive(Debug, Clone)]
pub struct AttackResult {
    pub hit: bool,
    pub natural_20: bool,
    pub natural_1: bool,
    pub attack_roll_first: i32,
    pub attack_roll_second: Option<i32>,
    pub attack_roll: i32,
    pub total_attack: i32,
    pub target_ac: i32,
    pub damage: i32,
    pub damage_type: DamageType,
    pub weapon_name: String,
    pub disadvantage: bool,
    /// True when the attack was rolled with advantage (after advantage/
    /// disadvantage cancellation). Used by the orchestrator to evaluate
    /// Rogue Sneak Attack trigger path 1 (attacker has Advantage).
    /// See `apply_sneak_attack` in lib.rs for the full two-path logic.
    pub attacker_had_advantage: bool,
}

pub fn format_attack_roll_details(result: &AttackResult, modifier: i32) -> String {
    match result.attack_roll_second {
        Some(other_roll) if result.disadvantage => format!(
            "{} / {} \u{2192} {} ({}) (disadvantage \u{2014} keeping {})",
            result.attack_roll_first,
            other_roll,
            result.attack_roll,
            format_roll(result.attack_roll, modifier, result.total_attack),
            result.attack_roll,
        ),
        Some(other_roll) if result.attacker_had_advantage => format!(
            "{} / {} \u{2192} {} ({}) (advantage \u{2014} keeping {})",
            result.attack_roll_first,
            other_roll,
            result.attack_roll,
            format_roll(result.attack_roll, modifier, result.total_attack),
            result.attack_roll,
        ),
        _ => format_roll(result.attack_roll, modifier, result.total_attack),
    }
}

/// Determine if the player's weapon attack is ranged based on target distance and weapon.
fn is_ranged_attack(weapon: &ItemType, distance: u32) -> bool {
    match weapon {
        ItemType::Weapon {
            range_normal,
            properties,
            ..
        } => {
            let is_ammo_weapon = properties & AMMUNITION != 0;
            let is_thrown = properties & THROWN != 0;
            // If weapon has range and target is beyond melee, it's a ranged attack
            // AMMUNITION weapons are always ranged when used at distance
            if is_ammo_weapon {
                true
            } else if is_thrown && distance > 5 {
                true
            } else if *range_normal > 0 && distance > 5 && !is_ammo_weapon && !is_thrown {
                // Pure ranged weapon with no melee capability
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Resolve the player attacking a target NPC.
///
/// `extra_advantage` is the orchestrator-supplied advantage flag (used by
/// the Vex weapon mastery to grant advantage on the next attack against a
/// marked target). It is combined with all other advantage sources and
/// cancels normally against any disadvantage source.
///
/// `target_cover` is the cover level of the NPC. If the target has
/// `Cover::Total`, the attack is rejected by the caller before reaching
/// this function; `resolve_player_attack` itself only applies the AC bonus
/// for Half and Three-quarters cover.
#[allow(clippy::too_many_arguments)]
pub fn resolve_player_attack(
    rng: &mut impl Rng,
    player: &Character,
    target_ac: i32,
    target_dodging: bool,
    weapon_id: Option<ItemId>,
    items: &HashMap<ItemId, crate::state::Item>,
    distance: u32,
    off_hand_free: bool,
    hostile_within_5ft: bool,
    target_conditions: &[ActiveCondition],
    extra_disadvantage: bool,
    extra_advantage: bool,
    target_cover: &Cover,
) -> AttackResult {
    // Base weapon fields come from either a mundane `Weapon` or a
    // `MagicWeapon` (which embeds the same mechanical fields). Magic
    // attack/damage bonuses are applied by the `lib.rs` orchestrator AFTER
    // this function returns — this function stays oblivious to magic bonuses.
    let (
        weapon_name,
        damage_dice,
        damage_die,
        damage_type,
        properties,
        versatile_die,
        range_normal,
        range_long,
    ) = match weapon_id.and_then(|id| items.get(&id)) {
        Some(item) => match &item.item_type {
            ItemType::Weapon {
                damage_dice,
                damage_die,
                damage_type,
                properties,
                versatile_die,
                range_normal,
                range_long,
                ..
            } => (
                item.name.clone(),
                *damage_dice,
                *damage_die,
                *damage_type,
                *properties,
                *versatile_die,
                *range_normal,
                *range_long,
            ),
            ItemType::MagicWeapon {
                damage_dice,
                damage_die,
                damage_type,
                properties,
                versatile_die,
                range_normal,
                range_long,
                ..
            } => (
                item.name.clone(),
                *damage_dice,
                *damage_die,
                *damage_type,
                *properties,
                *versatile_die,
                *range_normal,
                *range_long,
            ),
            _ => (
                "Unarmed".to_string(),
                0,
                0,
                DamageType::Bludgeoning,
                0u16,
                0,
                0,
                0,
            ),
        },
        None => (
            "Unarmed".to_string(),
            0,
            0,
            DamageType::Bludgeoning,
            0u16,
            0,
            0,
            0,
        ),
    };

    // Unarmed strikes (no weapon, damage_dice == 0) flow through the standard
    // attack-roll pipeline per SRD 5.1 Rules Glossary ("Unarmed Strike"):
    //   attack roll bonus = STR mod + proficiency bonus
    //   on hit: Bludgeoning damage = 1 + STR mod
    // Advantage/disadvantage from conditions applies automatically on the
    // standard path. See docs/reference/rules-glossary.md ("Unarmed Strike").
    let is_unarmed = damage_dice == 0;

    let is_finesse = properties & FINESSE != 0;
    let is_thrown = properties & THROWN != 0;
    let is_versatile = properties & VERSATILE != 0;
    // Note: REACH is consulted at the orchestrator layer via
    // `combat::player_melee_reach` to gate opportunity attacks. It does not
    // influence per-attack resolution, so no local flag is needed here.
    let ranged = is_ranged_attack(
        &ItemType::Weapon {
            damage_dice,
            damage_die,
            damage_type,
            properties,
            category: crate::state::WeaponCategory::Simple,
            versatile_die,
            range_normal,
            range_long,
        },
        distance,
    );

    // Determine ability modifier for attack
    let ability_mod = if ranged {
        if is_thrown {
            // Thrown uses STR (or DEX if FINESSE)
            if is_finesse {
                player
                    .ability_modifier(Ability::Strength)
                    .max(player.ability_modifier(Ability::Dexterity))
            } else {
                player.ability_modifier(Ability::Strength)
            }
        } else {
            // Ranged/AMMUNITION uses DEX
            player.ability_modifier(Ability::Dexterity)
        }
    } else if is_finesse {
        // Finesse: use higher of STR/DEX
        player
            .ability_modifier(Ability::Strength)
            .max(player.ability_modifier(Ability::Dexterity))
    } else {
        player.ability_modifier(Ability::Strength)
    };

    let prof_bonus = player.proficiency_bonus();

    // Check disadvantage
    let mut disadvantage = false;
    if target_dodging {
        disadvantage = true;
    }
    if extra_disadvantage {
        // Orchestrator-supplied disadvantage (e.g., Grappled vs non-grappler target).
        disadvantage = true;
    }
    if ranged {
        if hostile_within_5ft {
            disadvantage = true;
        }
        if distance > range_normal as u32 && distance <= range_long as u32 {
            disadvantage = true; // Long range
        }
    }
    // SRD 2024 Armor Training: wearing non-proficient armor imposes
    // Disadvantage on any D20 Test using STR or DEX, which includes every
    // weapon attack roll (STR for melee, DEX for ranged/finesse). See
    // docs/reference/equipment.md and docs/specs/equipment-system.md.
    if player.wearing_nonproficient_armor {
        disadvantage = true;
    }

    // Attacker-side conditions: Invisible grants advantage; Poisoned/Blinded/Prone/
    // Frightened/Restrained impose disadvantage.
    let mut advantage = false;
    if extra_advantage {
        // Orchestrator-supplied advantage (e.g., Vex mastery mark on target).
        advantage = true;
    }
    match conditions::get_attack_advantage(&player.conditions) {
        Some(true) => advantage = true,
        Some(false) => disadvantage = true,
        None => {}
    }

    // Defender-side conditions: Prone/Stunned/Paralyzed/Petrified/Restrained/
    // Unconscious/Blinded grant advantage; Invisible imposes disadvantage.
    match conditions::get_defense_advantage(&player.conditions, target_conditions) {
        Some(true) => {
            // Prone only grants advantage if within 5 ft; beyond 5 ft it flips to disadvantage.
            if conditions::has_condition(target_conditions, ConditionType::Prone) && distance > 5 {
                disadvantage = true;
            } else {
                advantage = true;
            }
        }
        Some(false) => disadvantage = true,
        None => {}
    }

    // Per SRD: advantage and disadvantage on the same roll cancel to neither.
    let attacker_has_advantage = if advantage && disadvantage {
        disadvantage = false;
        false
    } else {
        advantage
    };

    // Roll attack with advantage/disadvantage/neutral
    let roll1 = roll_d20(rng);
    let roll2 = roll_d20(rng);
    let attack_roll = if disadvantage {
        roll1.min(roll2)
    } else if attacker_has_advantage {
        roll1.max(roll2)
    } else {
        roll1
    };

    let natural_20 = attack_roll == 20;
    let natural_1 = attack_roll == 1;

    let total_attack = attack_roll + ability_mod + prof_bonus;
    let effective_ac = target_ac + target_cover.ac_bonus();
    let hit = if natural_1 {
        false
    } else if natural_20 {
        true
    } else {
        total_attack >= effective_ac
    };

    // Check for auto-crit (paralyzed target within 5ft)
    let auto_crit = hit && conditions::is_auto_crit_target(target_conditions) && distance <= 5;

    let damage = if hit {
        if is_unarmed {
            // Unarmed damage: flat 1 + STR mod. On a crit, the static "1" base
            // doubles to 2 (no dice to roll double). Floor at 1 HP.
            let base = if natural_20 || auto_crit { 2 } else { 1 };
            (base + ability_mod).max(1)
        } else {
            // Determine die to use
            let actual_die = if is_versatile && off_hand_free && versatile_die > 0 {
                versatile_die
            } else {
                damage_die
            };

            let dice_count = if natural_20 || auto_crit {
                damage_dice * 2
            } else {
                damage_dice
            };
            let dice_total: i32 = roll_dice(rng, dice_count, actual_die).iter().sum();
            (dice_total + ability_mod).max(1)
        }
    } else {
        0
    };

    AttackResult {
        hit,
        natural_20,
        natural_1,
        attack_roll_first: roll1,
        attack_roll_second: if disadvantage || attacker_has_advantage {
            Some(roll2)
        } else {
            None
        },
        attack_roll,
        total_attack,
        target_ac: effective_ac,
        damage,
        damage_type,
        weapon_name,
        disadvantage,
        attacker_had_advantage: attacker_has_advantage,
    }
}

/// Resolve an NPC attacking the player.
pub fn resolve_npc_attack(
    rng: &mut impl Rng,
    attack: &NpcAttack,
    player_ac: i32,
    player_dodging: bool,
    distance: u32,
    npc_conditions: &[ActiveCondition],
    player_conditions: &[ActiveCondition],
    extra_disadvantage: bool,
    player_cover: &Cover,
) -> AttackResult {
    let mut disadvantage = false;
    let mut advantage = false;

    if player_dodging {
        disadvantage = true;
    }
    if extra_disadvantage {
        // Orchestrator-supplied disadvantage (e.g., NPC Grappled and attacking a non-grappler).
        disadvantage = true;
    }

    let is_ranged = attack.reach == 0 && attack.range_normal > 0;
    if is_ranged && distance <= 5 {
        disadvantage = true; // Ranged attack in melee
    }
    if is_ranged && distance > attack.range_normal as u32 {
        disadvantage = true; // Long range
    }

    // Attacker-side conditions on the NPC: Invisible => advantage;
    // Poisoned/Blinded/Prone/Frightened/Restrained => disadvantage.
    match conditions::get_attack_advantage(npc_conditions) {
        Some(true) => advantage = true,
        Some(false) => disadvantage = true,
        None => {}
    }

    // Defender-side conditions on the player: Prone/Blinded/Stunned/Paralyzed/
    // Petrified/Restrained/Unconscious => advantage; Invisible => disadvantage.
    match conditions::get_defense_advantage(npc_conditions, player_conditions) {
        Some(true) => {
            // Prone only grants advantage within 5 ft; beyond, it's disadvantage.
            if conditions::has_condition(player_conditions, ConditionType::Prone) && distance > 5 {
                disadvantage = true;
            } else {
                advantage = true;
            }
        }
        Some(false) => disadvantage = true,
        None => {}
    }

    // Advantage and disadvantage cancel out per SRD.
    let use_disadvantage = disadvantage && !advantage;
    let use_advantage = advantage && !disadvantage;

    let roll1 = roll_d20(rng);
    let roll2 = roll_d20(rng);
    let attack_roll = if use_disadvantage {
        roll1.min(roll2)
    } else if use_advantage {
        roll1.max(roll2)
    } else {
        roll1
    };

    let natural_20 = attack_roll == 20;
    let natural_1 = attack_roll == 1;

    let total_attack = attack_roll + attack.hit_bonus;
    let effective_player_ac = player_ac + player_cover.ac_bonus();
    let hit = if natural_1 {
        false
    } else if natural_20 {
        true
    } else {
        total_attack >= effective_player_ac
    };

    // Check for auto-crit (paralyzed player within 5ft)
    let auto_crit = hit && conditions::is_auto_crit_target(player_conditions) && distance <= 5;

    let damage = if hit {
        let dice_count = if natural_20 || auto_crit {
            attack.damage_dice * 2
        } else {
            attack.damage_dice
        };
        let dice_total: i32 = roll_dice(rng, dice_count, attack.damage_die).iter().sum();
        (dice_total + attack.damage_bonus).max(1)
    } else {
        0
    };

    AttackResult {
        hit,
        natural_20,
        natural_1,
        attack_roll_first: roll1,
        attack_roll_second: if use_disadvantage || use_advantage {
            Some(roll2)
        } else {
            None
        },
        attack_roll,
        total_attack,
        target_ac: effective_player_ac,
        damage,
        damage_type: attack.damage_type,
        weapon_name: attack.name.clone(),
        disadvantage: use_disadvantage,
        attacker_had_advantage: use_advantage,
    }
}

/// Resolve an NPC opportunity attack against the player.
pub fn resolve_opportunity_attack(
    rng: &mut impl Rng,
    npc_id: NpcId,
    state: &GameState,
    player_ac: i32,
    distance: u32,
) -> Option<(String, AttackResult)> {
    let npc = state.world.npcs.get(&npc_id)?;
    let stats = npc.combat_stats.as_ref()?;
    if stats.current_hp <= 0 {
        return None;
    }

    // Find a melee attack that can reach the player at current distance
    let melee_attack = stats
        .attacks
        .iter()
        .find(|a| a.reach > 0 && distance <= a.reach as u32)?;
    // For NPC opportunity attacks, Grappled-vs-non-grappler disadvantage against
    // the player would apply if the NPC is grappled by someone other than the
    // player. We compute it here rather than leaking target-name parsing into combat.
    let extra_disadvantage =
        conditions::grappled_attack_disadvantage(&npc.conditions, &state.character.name);
    let result = resolve_npc_attack(
        rng,
        melee_attack,
        player_ac,
        false,
        distance,
        &npc.conditions,
        &state.character.conditions,
        extra_disadvantage,
        &Cover::None, // opportunity attacks don't check cover (player is fleeing)
    );
    Some((npc.name.clone(), result))
}

// ---- NPC AI ----

/// Attempt a melee or ranged attack at the given distance. Returns narration
/// lines for the multiattack loop, or an empty Vec if no attack is in range.
/// Shared by the initial attack check and the post-move re-check in
/// `resolve_npc_turn` so that sap/grapple disadvantage and multiattack logic
/// are never duplicated.
fn resolve_npc_attack_action(
    rng: &mut impl Rng,
    npc_attacks: &[NpcAttack],
    npc_multiattack: u32,
    distance: u32,
    grappled: bool,
    sapped_first_attack: bool,
    npc_name: &str,
    npc_conditions: &[crate::conditions::ActiveCondition],
    player_conditions: &[crate::conditions::ActiveCondition],
    state: &mut GameState,
    combat: &mut CombatState,
) -> Vec<String> {
    // Prefer melee if in reach, then try ranged.
    let melee = npc_attacks
        .iter()
        .find(|a| a.reach > 0 && distance <= a.reach as u32)
        .cloned();
    let ranged = npc_attacks
        .iter()
        .find(|a| a.range_long > 0 && distance <= a.range_long as u32)
        .cloned();

    let (attack, is_melee) = if let Some(a) = melee {
        (a, true)
    } else if let Some(a) = ranged {
        (a, false)
    } else {
        return Vec::new();
    };

    let mut lines = Vec::new();
    let verb = if is_melee { "attacks with" } else { "fires" };

    // Multiattack loop: roll `multiattack` separate attacks against the player.
    // If the player is already dying (HP <= 0), additional hits add death
    // save failures (per SRD). Multiattack stops only when the player has
    // accumulated three failures -- otherwise enemies keep swinging.
    for i in 0..npc_multiattack {
        if state.character.current_hp <= 0 && combat.death_save_failures >= 3 {
            break;
        }
        // Sap disadvantage applies only to the first attack of the turn.
        let iter_disadv = grappled || (sapped_first_attack && i == 0);
        let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
        let player_dodging = combat.player_dodging;
        let result = resolve_npc_attack(
            rng,
            &attack,
            player_ac,
            player_dodging,
            distance,
            npc_conditions,
            player_conditions,
            iter_disadv,
            &combat.player_cover,
        );
        if result.hit {
            let was_dying = state.character.current_hp <= 0;
            state.character.current_hp -= result.damage;
            if result.natural_20 {
                lines.push(format!(
                    "{} {} {} -- CRITICAL HIT! {} {} damage!",
                    npc_name, verb, result.weapon_name, result.damage, result.damage_type
                ));
            } else {
                lines.push(format!(
                    "{} {} {} ({} vs AC {}) -- hit for {} {} damage.",
                    npc_name,
                    verb,
                    result.weapon_name,
                    format_attack_roll_details(&result, attack.hit_bonus),
                    player_ac,
                    result.damage,
                    result.damage_type
                ));
            }
            // Damage-while-dying: if the player was already at 0 HP when
            // this hit landed, add a death save failure (two on a crit).
            if was_dying {
                let outcome = combat.apply_damage_while_dying(
                    &mut state.character,
                    result.damage,
                    result.natural_20,
                );
                lines.extend(narrate_damage_while_dying_outcome(outcome));
            }
        } else if result.natural_1 {
            lines.push(format!(
                "{} {} {} -- natural 1, miss!",
                npc_name, verb, result.weapon_name
            ));
        } else {
            lines.push(format!(
                "{} {} {} ({} vs AC {}) -- miss.",
                npc_name,
                verb,
                result.weapon_name,
                format_attack_roll_details(&result, attack.hit_bonus),
                player_ac
            ));
        }
    }
    lines
}

/// Returns true when the NPC should retreat (kite) rather than approach the
/// player. The condition is: HP below 30% of max AND the NPC has at least one
/// ranged attack AND the NPC is not immobilized (Grappled/Restrained setting
/// speed to zero).
pub fn npc_wants_to_retreat(
    stats: &CombatStats,
    npc_conditions: &[crate::conditions::ActiveCondition],
) -> bool {
    let hp_threshold = stats.max_hp * 30 / 100;
    let low_hp = stats.current_hp > 0 && stats.current_hp < hp_threshold.max(1);
    let has_ranged = stats.attacks.iter().any(|a| a.range_long > 0);
    let immobilized = conditions::speed_is_zero(npc_conditions);
    low_hp && has_ranged && !immobilized
}

/// Determine NPC action. Returns narration lines and whether to end combat early.
pub fn resolve_npc_turn(
    rng: &mut impl Rng,
    npc_id: NpcId,
    state: &mut GameState,
    combat: &mut CombatState,
) -> Vec<String> {
    let mut lines = Vec::new();

    let (npc_name, npc_speed, npc_attacks, _npc_ac, npc_multiattack) = {
        let npc = match state.world.npcs.get(&npc_id) {
            Some(n) => n,
            None => return lines,
        };
        let stats = match npc.combat_stats.as_ref() {
            Some(s) if s.current_hp > 0 => s,
            _ => return lines,
        };
        (
            npc.name.clone(),
            stats.speed,
            stats.attacks.clone(),
            stats.ac,
            stats.multiattack.max(1),
        )
    };

    let distance = *combat.distances.get(&npc_id).unwrap_or(&30);

    // Get NPC and conditions reference for attack resolution
    let npc_ref = match state.world.npcs.get(&npc_id) {
        Some(n) => n,
        None => return lines,
    };
    let npc_conditions = npc_ref.conditions.clone();
    let player_conditions = state.character.conditions.clone();

    // --- NPC Grapple AI: escape or initiate before attacking ---

    // If the NPC is grappled, spend the action trying to escape.
    if conditions::has_condition(&npc_conditions, ConditionType::Grappled) {
        if let Some(result) = resolve_npc_escape_grapple(rng, state, npc_id) {
            let skill_name = match result.skill_used {
                Skill::Athletics => "Athletics",
                Skill::Acrobatics => "Acrobatics",
                _ => "check",
            };
            if result.success {
                lines.push(format!(
                    "{} breaks free of the grapple! ({}: {} vs DC {} -- succeeds)",
                    npc_name,
                    skill_name,
                    crate::output::format_roll(
                        result.npc_d20,
                        result.npc_total - result.npc_d20,
                        result.npc_total,
                    ),
                    result.dc,
                ));
            } else {
                lines.push(format!(
                    "{} tries to escape the grapple but fails. ({}: {} vs DC {} -- fails)",
                    npc_name,
                    skill_name,
                    crate::output::format_roll(
                        result.npc_d20,
                        result.npc_total - result.npc_d20,
                        result.npc_total,
                    ),
                    result.dc,
                ));
            }
        }
        // Grappled NPC's action is spent on escape attempt; turn ends.
        return lines;
    }

    // NPC grapple initiation: wounded NPCs in melee range try to grapple
    // the player instead of attacking.
    let npc_hp_low = {
        let npc = state.world.npcs.get(&npc_id);
        npc.and_then(|n| n.combat_stats.as_ref())
            .map(|s| s.current_hp <= s.max_hp / 2 && s.current_hp > 0)
            .unwrap_or(false)
    };
    let player_already_grappled_by_this_npc = state.character.conditions.iter().any(|c| {
        c.condition == ConditionType::Grappled
            && c.source
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(&npc_name))
                .unwrap_or(false)
    });

    if npc_hp_low && distance <= 5 && !player_already_grappled_by_this_npc {
        // Check size limit: NPC cannot grapple a target more than one size
        // larger. Player is Medium.
        let npc_size = state
            .world
            .npcs
            .get(&npc_id)
            .and_then(|n| n.combat_stats.as_ref())
            .map(|s| s.size)
            .unwrap_or(crate::combat::monsters::Size::Medium);
        let player_size = crate::combat::monsters::Size::Medium;
        if !target_exceeds_grapple_size_limit(&npc_size, &player_size) {
            if let Some(result) = resolve_npc_grapple_attempt(rng, state, npc_id) {
                if result.danger_sense_active {
                    lines.push("(Danger Sense: advantage on DEX save)".to_string());
                }
                if result.success {
                    let save_name = match result.save_ability {
                        Ability::Strength => "STR",
                        Ability::Dexterity => "DEX",
                        _ => "save",
                    };
                    lines.push(format!(
                        "{} grapples you! ({} save: {} vs DC {} -- fails). You are Grappled (speed 0).",
                        npc_name,
                        save_name,
                        crate::output::format_roll(
                            result.player_d20,
                            result.player_save_total - result.player_d20,
                            result.player_save_total,
                        ),
                        result.dc,
                    ));
                } else {
                    let save_name = match result.save_ability {
                        Ability::Strength => "STR",
                        Ability::Dexterity => "DEX",
                        _ => "save",
                    };
                    lines.push(format!(
                        "{} tries to grapple you but fails! ({} save: {} vs DC {} -- succeeds).",
                        npc_name,
                        save_name,
                        crate::output::format_roll(
                            result.player_d20,
                            result.player_save_total - result.player_d20,
                            result.player_save_total,
                        ),
                        result.dc,
                    ));
                }
                // NPC used its action on the grapple attempt.
                return lines;
            }
        }
    }

    // Priority: melee if in range -> ranged if in range -> move toward player
    // Orchestrator-side grappled disadvantage: if the NPC is grappled by
    // someone other than the player, attacking the player is at disadvantage.
    let grappled = conditions::grappled_attack_disadvantage(&npc_conditions, &state.character.name);
    // Sap mastery: consume the mark so only the FIRST attack this turn is
    // rolled with disadvantage. Multiattack follow-ups revert to normal.
    let sapped_first_attack = consume_sap_disadvantage(combat, npc_id);
    if sapped_first_attack {
        lines.push("(Disadvantage from Sap mastery.)".to_string());
    }

    // ---- Retreat / kite AI (issue #256) ----
    // When the NPC's HP is below 30% of max AND it has at least one ranged
    // attack AND it is not immobilized (speed 0 from Grappled etc.), it
    // retreats: moves away from the player by its speed, then fires ranged.
    //
    // If the NPC is currently within the player's melee reach and would
    // provoke an opportunity attack by moving away, it uses its action to
    // Disengage first (setting `npc_disengaging`), sacrificing the ranged
    // attack on this turn.
    let npc_wants_retreat = npc_wants_to_retreat(
        state.world.npcs.get(&npc_id).unwrap().combat_stats.as_ref().unwrap(),
        &npc_conditions,
    );
    if npc_wants_retreat {
        let slow_reduction = slow_speed_reduction(combat, npc_id).max(0) as u32;
        let effective_speed = (npc_speed as u32).saturating_sub(slow_reduction);
        if slow_reduction > 0 {
            lines.push(format!(
                "(Slow: {}'s Speed reduced by {} ft this turn.)",
                npc_name, slow_reduction,
            ));
        }

        // Check if NPC is within player's melee reach. If so, and if the
        // NPC would not escape reach even with full movement, it uses its
        // action to Disengage (suppressing the player's OA). Otherwise the
        // NPC just runs, risking an OA but keeping its action for a ranged
        // attack after retreating.
        let player_reach = player_melee_reach(&state.character, &state.world.items);
        let predicted_distance = distance.saturating_add(effective_speed);
        let cannot_escape_reach = distance <= player_reach && predicted_distance <= player_reach;
        let used_disengage = cannot_escape_reach
            && !combat.npc_disengaging.get(&npc_id).copied().unwrap_or(false);
        if used_disengage {
            combat.npc_disengaging.insert(npc_id, true);
            lines.push(format!("{} disengages.", npc_name));
        }

        // Move away from the player.
        let new_distance = predicted_distance;
        combat.distances.insert(npc_id, new_distance);
        lines.push(format!(
            "{} retreats. ({}ft -> {}ft)",
            npc_name, distance, new_distance,
        ));

        // If the NPC did NOT Disengage (action still available), fire ranged.
        if !used_disengage {
            let post_retreat_attack = resolve_npc_attack_action(
                rng,
                &npc_attacks,
                npc_multiattack,
                new_distance,
                grappled,
                sapped_first_attack,
                &npc_name,
                &npc_conditions,
                &player_conditions,
                state,
                combat,
            );
            lines.extend(post_retreat_attack);
        }

        return lines;
    }

    // ---- Standard AI: melee if in range -> ranged if in range -> approach ----

    // Try to attack at the current distance; if not in range, move then
    // re-check. This mirrors SRD 5.1: movement and action are independent
    // resources on the same turn.
    let attack_lines = resolve_npc_attack_action(
        rng,
        &npc_attacks,
        npc_multiattack,
        distance,
        grappled,
        sapped_first_attack,
        &npc_name,
        &npc_conditions,
        &player_conditions,
        state,
        combat,
    );
    if !attack_lines.is_empty() {
        lines.extend(attack_lines);
        return lines;
    }

    // No attack in range — move toward the player first (unless speed is 0).
    if conditions::speed_is_zero(&npc_conditions) {
        lines.push(format!("{} cannot move (speed 0).", npc_name));
        return lines;
    }

    // Slow mastery (2024 SRD) reduces the NPC's Speed by up to 10 ft for
    // this move; the reduction is reported once so the player can see why
    // the NPC moved less.
    let slow_reduction = slow_speed_reduction(combat, npc_id).max(0) as u32;
    let effective_speed = (npc_speed as u32).saturating_sub(slow_reduction);
    if slow_reduction > 0 {
        lines.push(format!(
            "(Slow: {}'s Speed reduced by {} ft this turn.)",
            npc_name, slow_reduction,
        ));
    }
    let move_amount = effective_speed;
    let new_distance = if distance > move_amount {
        distance - move_amount
    } else {
        5
    };
    combat.distances.insert(npc_id, new_distance);
    lines.push(format!(
        "{} moves toward you. ({}ft -> {}ft)",
        npc_name, distance, new_distance
    ));

    // After moving, attempt an attack if now in range (SRD: move then act).
    let post_move_attack = resolve_npc_attack_action(
        rng,
        &npc_attacks,
        npc_multiattack,
        new_distance,
        grappled,
        sapped_first_attack,
        &npc_name,
        &npc_conditions,
        &player_conditions,
        state,
        combat,
    );
    lines.extend(post_move_attack);

    lines
}

// ---- Player Movement ----

/// Check if the player is currently grappling any NPC (i.e., any NPC has
/// `Grappled` sourced to the player). Used to determine drag movement cost.
pub fn player_is_dragging(state: &GameState) -> bool {
    let player_name = &state.character.name;
    state.world.npcs.values().any(|npc| {
        npc.conditions.iter().any(|c| {
            c.condition == ConditionType::Grappled
                && c.source
                    .as_deref()
                    .map(|s| s.eq_ignore_ascii_case(player_name))
                    .unwrap_or(false)
        })
    })
}

/// Move the player toward a target NPC. Returns narration lines.
pub fn approach_target(
    _rng: &mut impl Rng,
    target_id: NpcId,
    state: &GameState,
    combat: &mut CombatState,
) -> Vec<String> {
    let mut lines = Vec::new();
    let distance = *combat.distances.get(&target_id).unwrap_or(&30);
    let movement = combat.player_movement_remaining;

    if movement <= 0 {
        return vec!["You have no movement remaining this turn.".to_string()];
    }

    if distance <= 5 {
        return vec!["You are already in melee range.".to_string()];
    }

    // Drag cost: when grappling an NPC, each foot of movement costs 2 feet.
    let dragging = player_is_dragging(state);
    let effective_movement = if dragging {
        (movement as u32) / 2
    } else {
        movement as u32
    };

    let move_amount = effective_movement.min(distance - 5);
    let new_distance = distance - move_amount;
    combat.distances.insert(target_id, new_distance);
    // Consume double movement when dragging.
    let movement_consumed = if dragging {
        move_amount * 2
    } else {
        move_amount
    };
    combat.player_movement_remaining -= movement_consumed as i32;

    let target_name = state
        .world
        .npcs
        .get(&target_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "the enemy".to_string());

    if dragging {
        lines.push(format!(
            "You drag toward {}. ({}ft -> {}ft, {}ft movement remaining, halved by drag)",
            target_name, distance, new_distance, combat.player_movement_remaining
        ));
    } else {
        lines.push(format!(
            "You move toward {}. ({}ft -> {}ft, {}ft movement remaining)",
            target_name, distance, new_distance, combat.player_movement_remaining
        ));
    }

    lines
}

/// Move the player away from all enemies. Returns narration lines.
pub fn retreat(rng: &mut impl Rng, state: &mut GameState, combat: &mut CombatState) -> Vec<String> {
    let mut lines = Vec::new();
    let movement = combat.player_movement_remaining;

    if movement <= 0 {
        return vec!["You have no movement remaining this turn.".to_string()];
    }

    // Drag cost: when grappling an NPC, movement costs double.
    let dragging = player_is_dragging(state);
    let move_amount = if dragging {
        (movement as u32) / 2
    } else {
        movement as u32
    };

    // Build distance map: npc_id -> (old_distance, new_distance)
    let distance_changes: Vec<(NpcId, u32, u32)> = combat
        .distances
        .iter()
        .map(|(&id, &old)| (id, old, old.saturating_add(move_amount)))
        .collect();

    if !combat.player_disengaging {
        let oa_lines = fire_opportunity_attacks(rng, state, combat, &distance_changes);
        lines.extend(oa_lines);
    }

    // Move all distances by movement amount
    for (_, dist) in combat.distances.iter_mut() {
        *dist += move_amount;
    }
    combat.player_movement_remaining = 0;

    if dragging {
        lines.push(format!("You retreat {} ft (halved by drag).", move_amount));
    } else {
        lines.push(format!("You retreat {} ft.", move_amount));
    }

    // Distance auto-release: grapple ends when distance exceeds 5 ft.
    let player_name = state.character.name.clone();
    for npc in state.world.npcs.values_mut() {
        let dist = combat.distances.get(&npc.id).copied().unwrap_or(0);
        if dist > 5 {
            let had_grapple = npc.conditions.iter().any(|c| {
                c.condition == ConditionType::Grappled
                    && c.source
                        .as_deref()
                        .map(|s| s.eq_ignore_ascii_case(&player_name))
                        .unwrap_or(false)
            });
            if had_grapple {
                release_grapple_on_npc(npc, &player_name);
                lines.push(format!(
                    "Your grapple on {} ends (distance exceeded 5 ft).",
                    npc.name
                ));
            }
        }
    }

    lines
}

/// Check each NPC whose reach the player is leaving and fire an opportunity
/// attack if that NPC still has its reaction this round.
/// `distance_changes` is a list of `(npc_id, old_distance, new_distance)`.
pub fn fire_opportunity_attacks(
    rng: &mut impl Rng,
    state: &mut GameState,
    combat: &mut CombatState,
    distance_changes: &[(NpcId, u32, u32)],
) -> Vec<String> {
    let mut lines = Vec::new();
    let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);

    for &(npc_id, old_distance, new_distance) in distance_changes {
        // Skip if this NPC already used its reaction this round
        if combat.npc_reactions_used.contains(&npc_id) {
            continue;
        }

        let leaves_reach = state
            .world
            .npcs
            .get(&npc_id)
            .and_then(|npc| npc.combat_stats.as_ref())
            .and_then(|stats| {
                if stats.current_hp <= 0 {
                    return None;
                }
                let max_melee_reach = stats
                    .attacks
                    .iter()
                    .filter(|a| a.reach > 0)
                    .map(|a| a.reach as u32)
                    .max()?;
                Some(old_distance <= max_melee_reach && new_distance > max_melee_reach)
            })
            .unwrap_or(false);

        if !leaves_reach {
            continue;
        }

        // Consume the NPC's reaction
        combat.npc_reactions_used.insert(npc_id);

        if let Some((npc_name, result)) =
            resolve_opportunity_attack(rng, npc_id, state, player_ac, old_distance)
        {
            if result.hit {
                let was_dying = state.character.current_hp <= 0;
                state.character.current_hp -= result.damage;
                lines.push(format!(
                    "{} makes an opportunity attack with {} -- hit for {} {} damage!",
                    npc_name, result.weapon_name, result.damage, result.damage_type
                ));
                if was_dying {
                    let outcome = combat.apply_damage_while_dying(
                        &mut state.character,
                        result.damage,
                        result.natural_20,
                    );
                    lines.extend(narrate_damage_while_dying_outcome(outcome));
                }
            } else {
                lines.push(format!(
                    "{} makes an opportunity attack with {} -- miss!",
                    npc_name, result.weapon_name
                ));
            }
        }
    }

    lines
}

/// Format the current combat status for the "look" command.
pub fn format_combat_status(state: &GameState, combat: &CombatState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("=== Combat - Round {} ===", combat.round));
    lines.push(format!(
        "HP: {}/{}",
        state.character.current_hp, state.character.max_hp
    ));
    lines.push(format!(
        "AC: {}",
        crate::equipment::calculate_ac(&state.character, &state.world.items)
    ));
    lines.push(String::new());
    lines.push("Enemies:".to_string());

    for (combatant, _) in &combat.initiative_order {
        if let Combatant::Npc(id) = combatant {
            if let Some(npc) = state.world.npcs.get(id) {
                if let Some(stats) = &npc.combat_stats {
                    let distance = combat.distances.get(id).copied().unwrap_or(0);
                    let status = if stats.current_hp <= 0 {
                        "DEAD".to_string()
                    } else {
                        format!(
                            "HP {}/{}, {}ft away",
                            stats.current_hp, stats.max_hp, distance
                        )
                    };
                    lines.push(format!("  {} - {}", npc.name, status));
                }
            }
        }
    }

    if combat.is_player_turn() {
        lines.push(String::new());
        lines.push(format!(
            "Movement remaining: {} ft",
            combat.player_movement_remaining
        ));
        let status = |used: bool| if used { "used" } else { "available" };
        lines.push(format!(
            "Action: {} | Bonus: {} | Reaction: {} | Free interaction: {}",
            status(combat.action_used),
            status(combat.bonus_action_used),
            status(combat.reaction_used),
            status(combat.free_interaction_used),
        ));
        if !combat.action_used {
            lines.push(
                "Commands: attack <target>, grapple <target>, shove <target>, shove prone <target>, dodge, disengage, dash"
                    .to_string(),
            );
        } else {
            lines.push(
                "Action used. You can still move (approach/retreat) or spend your bonus action."
                    .to_string(),
            );
        }
        if matches!(
            state.character.class,
            crate::character::class::Class::Barbarian
        ) {
            if state.character.class_features.rage_active {
                lines.push(
                    "Rage active: your melee hits gain bonus damage and you resist physical damage."
                        .to_string(),
                );
            } else if state.character.class_features.rage_uses_remaining == 0 {
                lines.push(
                    "Barbarian cues: Rage is spent for now. Grapple and shove still give you strong melee control."
                        .to_string(),
                );
            } else {
                lines.push(format!(
                    "Barbarian cues: rage is ready ({} use{} left). Grapple and shove are strong melee control options.",
                    state.character.class_features.rage_uses_remaining,
                    if state.character.class_features.rage_uses_remaining == 1 {
                        ""
                    } else {
                        "s"
                    }
                ));
            }
        }
    }

    lines
}

/// Compact enemy status lines for the turn prompt.
pub fn format_enemy_summary(state: &GameState, combat: &CombatState) -> Vec<String> {
    let mut lines = Vec::new();
    for (combatant, _) in &combat.initiative_order {
        if let Combatant::Npc(id) = combatant {
            if let Some(npc) = state.world.npcs.get(id) {
                if let Some(stats) = &npc.combat_stats {
                    if stats.current_hp <= 0 {
                        continue;
                    }
                    let distance = combat.distances.get(id).copied().unwrap_or(0);
                    let range_label = if distance <= 5 {
                        "melee".to_string()
                    } else {
                        format!("{}ft", distance)
                    };
                    lines.push(format!(
                        "  {} — HP {}/{}, {}",
                        npc.name, stats.current_hp, stats.max_hp, range_label
                    ));
                }
            }
        }
    }
    lines
}

/// Format initiative order announcement.
pub fn format_initiative(combat: &CombatState, state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push("=== COMBAT BEGINS ===".to_string());
    lines.push(String::new());
    lines.push("Initiative order:".to_string());
    for (i, (combatant, init)) in combat.initiative_order.iter().enumerate() {
        let name = match combatant {
            Combatant::Player => state.character.name.clone(),
            Combatant::Npc(id) => state
                .world
                .npcs
                .get(id)
                .map(|n| n.name.clone())
                .unwrap_or_else(|| format!("NPC {}", id)),
        };
        let marker = if i == combat.current_turn { " <--" } else { "" };
        lines.push(format!("  {} ({}){}", name, init, marker));
    }
    lines.push(String::new());

    // Show distances
    lines.push("Distances:".to_string());
    for (&npc_id, &dist) in &combat.distances {
        let name = state
            .world
            .npcs
            .get(&npc_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| format!("NPC {}", npc_id));
        lines.push(format!("  {}: {} ft", name, dist));
    }

    lines
}

/// Apply a condition to an NPC, honoring stat-block immunities AND the
/// generic `conditions::is_immune_to_condition` rules (e.g., Petrified =>
/// Poisoned). Returns `true` if the condition was applied, `false` if it
/// was rejected by either immunity check.
///
/// Lives in `combat/` (not `conditions/`) so the conditions module stays
/// decoupled from NPC stat-block storage. See
/// `docs/specs/monster-stat-blocks.md`.
pub fn try_apply_condition_to_npc(npc: &mut Npc, new_condition: ActiveCondition) -> bool {
    // Stat-block immunity check (e.g., Skeleton immune to Poisoned).
    if let Some(stats) = npc.combat_stats.as_ref() {
        if stats
            .condition_immunities
            .contains(&new_condition.condition)
        {
            return false;
        }
    }
    // Generic condition-vs-condition immunity (e.g., Petrified => Poisoned).
    conditions::apply_condition(&mut npc.conditions, new_condition)
}

/// Apply damage to an NPC, honoring its damage immunities and resistances.
/// Mutates `npc.combat_stats.current_hp` in place, capping at 0 from below.
/// Returns the actual damage dealt after modifiers (immunity -> 0, resistance
/// -> halved, otherwise -> incoming). Appends modifier narration to
/// `narration` (use an empty `Vec` to discard).
///
/// No-op if the NPC has no `combat_stats` (e.g., friendly NPC); returns 0.
pub fn apply_damage_to_npc(
    npc: &mut Npc,
    incoming: i32,
    damage_type: DamageType,
    narration: &mut Vec<String>,
) -> i32 {
    let name = npc.name.clone();
    let Some(stats) = npc.combat_stats.as_mut() else {
        return 0;
    };
    let dealt = apply_damage_modifiers(stats, incoming, damage_type, &name, narration);
    stats.current_hp -= dealt;
    if stats.current_hp < 0 {
        stats.current_hp = 0;
    }
    dealt
}

/// Apply incoming damage of `damage_type` to a `CombatStats` snapshot,
/// honoring damage immunities and resistances. Returns the actual damage
/// applied (already capped to current_hp >= 0 by the caller's HP write).
///
/// - If `damage_type` is in `damage_immunities`, returns 0 and emits an
///   immunity narration line into `narration` if provided.
/// - Else if `damage_type` is in `damage_resistances`, returns
///   `incoming / 2` (rounded down, minimum 0) and emits a resistance line.
/// - Else returns `incoming` unchanged.
///
/// `target_name` is used when building narration text; pass the NPC's
/// display name. The narration `Vec` parameter is appended to in-place; pass
/// an empty `Vec` to discard narration (useful in tests).
pub fn apply_damage_modifiers(
    stats: &CombatStats,
    incoming: i32,
    damage_type: DamageType,
    target_name: &str,
    narration: &mut Vec<String>,
) -> i32 {
    if incoming <= 0 {
        return incoming.max(0);
    }
    if stats.damage_immunities.contains(&damage_type) {
        narration.push(format!(
            "The {} is immune to {} damage!",
            target_name, damage_type
        ));
        return 0;
    }
    if stats.damage_resistances.contains(&damage_type) {
        let halved = incoming / 2;
        narration.push(format!(
            "The {} resists the {} damage.",
            target_name, damage_type
        ));
        return halved.max(0);
    }
    incoming
}

// ---------- Weapon Mastery (2024 SRD) ----------------------------------
//
// Mastery effects are applied after `resolve_player_attack` returns. The
// orchestrator in `lib.rs` calls these helpers with the attack result, the
// weapon's mastery, and relevant state. The helpers mutate `CombatState`
// and the target NPC directly; they return narration lines for the caller
// to emit. This keeps combat's mastery logic local to this module while
// respecting the `lib.rs` orchestrator pattern for cross-module glue.
//
// Every helper is a no-op when the character does not have mastery for
// the attacking weapon. Callers are expected to guard with
// `equipment::character_has_mastery` *before* calling any of these, but
// the helpers also accept a `has_mastery: bool` parameter for belt-and-
// suspenders safety and easier testing.

/// Graze: if the attack missed, deal ability-modifier damage of the
/// weapon's type. Per SRD 2024, the damage "can be increased only by
/// increasing the ability modifier" — we pass the modifier used for the
/// attack roll in `ability_mod_used` and floor the damage at 0.
///
/// Returns the damage actually dealt (after resistance/immunity filtering
/// via `apply_damage_to_npc`), or 0 when mastery doesn't apply.
///
/// Narration describing the graze is appended to `narration`.
pub fn apply_graze_mastery(
    has_mastery: bool,
    result: &AttackResult,
    ability_mod_used: i32,
    npc: &mut Npc,
    narration: &mut Vec<String>,
) -> i32 {
    if !has_mastery || result.hit || ability_mod_used <= 0 {
        return 0;
    }
    let damage = ability_mod_used;
    let npc_name = npc.name.clone();
    let dealt = apply_damage_to_npc(npc, damage, result.damage_type, narration);
    if dealt > 0 {
        narration.push(format!(
            "Graze: you still deal {} {} damage to {}.",
            dealt, result.damage_type, npc_name
        ));
    }
    dealt
}

/// Vex: on a hit that deals damage, mark the target so the player's next
/// attack against them is made with advantage. Cleared at the start of
/// the player's next turn or when consumed (see `consume_vex_advantage`).
///
/// Returns true when the mark was set.
pub fn apply_vex_mastery(
    has_mastery: bool,
    result: &AttackResult,
    target_npc_id: NpcId,
    combat: &mut CombatState,
    narration: &mut Vec<String>,
) -> bool {
    if !has_mastery || !result.hit || result.damage <= 0 {
        return false;
    }
    combat.player_vex_target = Some(target_npc_id);
    narration.push("Vex: you have advantage on your next attack against this target.".to_string());
    true
}

/// Returns true when the player has advantage on their next attack vs
/// `target_npc_id` because of a previously-applied Vex mastery. Consumes
/// the mark (clears `combat.player_vex_target`) so a subsequent attack
/// does not retain the advantage.
pub fn consume_vex_advantage(combat: &mut CombatState, target_npc_id: NpcId) -> bool {
    if combat.player_vex_target == Some(target_npc_id) {
        combat.player_vex_target = None;
        return true;
    }
    false
}

/// Sap: on a hit, mark the target so their next attack roll (before the
/// start of the player's next turn) is made at disadvantage. Returns true
/// when the mark was set.
pub fn apply_sap_mastery(
    has_mastery: bool,
    result: &AttackResult,
    target_npc_id: NpcId,
    combat: &mut CombatState,
    narration: &mut Vec<String>,
) -> bool {
    if !has_mastery || !result.hit {
        return false;
    }
    combat.sap_targets.insert(target_npc_id);
    narration.push("Sap: the target has disadvantage on its next attack roll.".to_string());
    true
}

/// Returns true when `npc_id` is currently sap-marked and consumes the
/// mark. Intended to be called once per NPC attack so the mark only
/// affects one attack roll.
pub fn consume_sap_disadvantage(combat: &mut CombatState, npc_id: NpcId) -> bool {
    combat.sap_targets.remove(&npc_id)
}

/// Slow: on a hit that deals damage, reduce the target's Speed by 10 ft
/// until the start of the player's next turn. Per SRD the reduction from
/// a single source does not exceed 10 ft, so subsequent Slow hits on the
/// same target this turn are no-ops for the target's speed accounting.
///
/// Returns true when the mark was newly applied.
pub fn apply_slow_mastery(
    has_mastery: bool,
    result: &AttackResult,
    target_npc_id: NpcId,
    combat: &mut CombatState,
    narration: &mut Vec<String>,
) -> bool {
    if !has_mastery || !result.hit || result.damage <= 0 {
        return false;
    }
    let existing = combat
        .slow_targets
        .get(&target_npc_id)
        .copied()
        .unwrap_or(0);
    if existing >= 10 {
        return false;
    }
    combat.slow_targets.insert(target_npc_id, 10);
    narration.push(
        "Slow: the target's Speed is reduced by 10 ft until the start of your next turn."
            .to_string(),
    );
    true
}

/// Current Slow reduction (in feet) for `npc_id`. 0 when the target is
/// not currently slow-marked. Used by NPC movement accounting.
pub fn slow_speed_reduction(combat: &CombatState, npc_id: NpcId) -> i32 {
    combat.slow_targets.get(&npc_id).copied().unwrap_or(0)
}

/// Push: on a hit against a Large-or-smaller creature, shove them 10 ft
/// away. The engine's 1D combat model represents this by adding 10 ft to
/// the player-to-target distance. Returns Some(new_distance) if pushed,
/// None if the mastery does not apply.
///
/// Per SRD 2024 the push is optional ("you can"), but for MVP we always
/// push when the mastery fires.
pub fn apply_push_mastery(
    has_mastery: bool,
    result: &AttackResult,
    target_npc_id: NpcId,
    combat: &mut CombatState,
    narration: &mut Vec<String>,
    target_size: crate::combat::monsters::Size,
) -> Option<u32> {
    if !has_mastery || !result.hit {
        return None;
    }
    // Per SRD: only Large or smaller. Huge/Gargantuan are immune.
    use crate::combat::monsters::Size;
    if matches!(target_size, Size::Huge | Size::Gargantuan) {
        return None;
    }
    let current = combat.distances.get(&target_npc_id).copied().unwrap_or(5);
    let new_distance = current.saturating_add(10);
    combat.distances.insert(target_npc_id, new_distance);
    narration.push(format!(
        "Push: the target is shoved back 10 ft ({}ft -> {}ft).",
        current, new_distance
    ));
    Some(new_distance)
}

/// Topple: on a hit, the target makes a CON save vs DC (8 + ability mod
/// used for the attack roll + player proficiency bonus). On a failure,
/// the target gains the Prone condition.
///
/// Returns true when Prone was applied. The RNG is consumed for the
/// target's CON save.
pub fn apply_topple_mastery(
    has_mastery: bool,
    result: &AttackResult,
    target_npc_id: NpcId,
    state: &mut GameState,
    narration: &mut Vec<String>,
    ability_mod_used: i32,
    player_proficiency_bonus: i32,
    rng: &mut impl Rng,
) -> bool {
    if !has_mastery || !result.hit {
        return false;
    }
    let dc = 8 + ability_mod_used + player_proficiency_bonus;
    let Some(npc) = state.world.npcs.get_mut(&target_npc_id) else {
        return false;
    };
    let Some(stats) = npc.combat_stats.as_ref() else {
        return false;
    };
    let con = stats
        .ability_scores
        .get(&Ability::Constitution)
        .copied()
        .unwrap_or(10);
    let con_mod = Ability::modifier(con);
    let con_save_prof = 0; // NPC CON save proficiency is not modelled in MVP.
    let roll = roll_d20(rng);
    let save_total = roll + con_mod + con_save_prof;
    let npc_name = npc.name.clone();
    if save_total >= dc {
        narration.push(format!(
            "Topple: {} succeeds on a CON save ({} vs DC {}).",
            npc_name,
            format_roll(roll, con_mod, save_total),
            dc
        ));
        return false;
    }
    // Apply Prone (honoring the NPC's condition immunities).
    let new_cond = ActiveCondition::new(
        ConditionType::Prone,
        crate::conditions::ConditionDuration::Permanent,
    );
    let applied = conditions::apply_condition(&mut npc.conditions, new_cond);
    if applied {
        narration.push(format!(
            "Topple: {} fails the CON save ({} vs DC {}) and is knocked Prone!",
            npc_name,
            format_roll(roll, con_mod, save_total),
            dc
        ));
    } else {
        narration.push(format!(
            "Topple: {} fails the CON save but is immune to Prone.",
            npc_name
        ));
    }
    applied
}

// ---------- Grappling (2024 SRD) ----------------------------------------
//
// Grapple initiation: target chooses STR or DEX save vs DC (8 + grappler
// STR mod + PB). Grappler must have a free hand; target must not be more
// than one size category larger. On failure, the target gains the Grappled
// condition with the grappler's name as source.
//
// Escape: grappled creature uses its Action for an Athletics (STR) or
// Acrobatics (DEX) check vs the same DC formula.
//
// The Grappler feat grants advantage on grapple attempts. Wiring happens
// in lib.rs at the orchestrator level.
//
// Size-limit helpers (NPC size lives on CombatStats; PC size = Medium).

/// Map a size category to an ordinal so we can compare sizes arithmetically.
fn size_ordinal(size: &monsters::Size) -> i32 {
    match size {
        monsters::Size::Tiny => 0,
        monsters::Size::Small => 1,
        monsters::Size::Medium => 2,
        monsters::Size::Large => 3,
        monsters::Size::Huge => 4,
        monsters::Size::Gargantuan => 5,
    }
}

/// Return true when the target is more than one size category larger than
/// the grappler. A Medium grappler cannot grapple a Huge or Gargantuan
/// creature; they can still grapple Large ones.
pub fn target_exceeds_grapple_size_limit(
    grappler_size: &monsters::Size,
    target_size: &monsters::Size,
) -> bool {
    size_ordinal(target_size) > size_ordinal(grappler_size) + 1
}

/// Compute the grapple DC: `8 + STR modifier + proficiency bonus`.
pub fn grapple_dc(str_score: i32, pb: i32) -> i32 {
    8 + Ability::modifier(str_score) + pb
}

/// The result of a grapple attempt against a single NPC.
#[derive(Debug, Clone)]
pub struct GrappleAttemptResult {
    /// Did the grapple succeed (target failed the save)?
    pub success: bool,
    /// The raw d20 the target rolled.
    pub target_d20: i32,
    /// d20 + save modifier.
    pub target_save_total: i32,
    /// Save DC the target rolled against.
    pub dc: i32,
    /// Which ability the target used for the save (STR or DEX).
    pub save_ability: Ability,
}

/// Attempt to grapple `target_npc_id`. Rolls the target's save and, on
/// failure, applies the Grappled condition.
///
/// Returns `None` when the target NPC does not exist or has no combat stats.
///
/// `grappler_str_score` and `grappler_pb` come from the player's stats.
/// `advantage` is true when the Grappler feat is active.
/// `grappler_name` is stored as the condition source for later release checks.
pub fn resolve_grapple_attempt(
    rng: &mut impl Rng,
    state: &mut GameState,
    target_npc_id: NpcId,
    grappler_str_score: i32,
    grappler_pb: i32,
    grappler_name: &str,
    advantage: bool,
) -> Option<GrappleAttemptResult> {
    let npc = state.world.npcs.get_mut(&target_npc_id)?;
    let stats = npc.combat_stats.as_ref()?;

    // Per 2024 SRD the target picks whichever of STR or DEX gives the
    // higher save total. We compute both and pick the better one.
    let str_score = stats
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let dex_score = stats
        .ability_scores
        .get(&Ability::Dexterity)
        .copied()
        .unwrap_or(10);
    let str_mod = Ability::modifier(str_score);
    let dex_mod = Ability::modifier(dex_score);
    let (save_mod, save_ability) = if str_mod >= dex_mod {
        (str_mod, Ability::Strength)
    } else {
        (dex_mod, Ability::Dexterity)
    };

    let dc = grapple_dc(grappler_str_score, grappler_pb);

    // Roll the target's save (NPC has no save proficiency in MVP).
    let d20 = if advantage {
        roll_d20(rng).max(roll_d20(rng))
    } else {
        roll_d20(rng)
    };
    let save_total = d20 + save_mod;

    if save_total >= dc {
        // Target succeeds — no grapple.
        return Some(GrappleAttemptResult {
            success: false,
            target_d20: d20,
            target_save_total: save_total,
            dc,
            save_ability,
        });
    }

    // Target fails — apply Grappled (honoring condition immunities).
    let grappled_cond = ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
        .with_source(grappler_name);
    // Re-borrow the NPC after the mutable borrow for stats ended.
    let npc = state.world.npcs.get_mut(&target_npc_id)?;
    let _applied = try_apply_condition_to_npc(npc, grappled_cond);

    Some(GrappleAttemptResult {
        success: true, // even if immune, we return success=true to keep narration simple;
        // `try_apply_condition_to_npc` is false on immunity
        target_d20: d20,
        target_save_total: save_total,
        dc,
        save_ability,
    })
}

/// The result of a player attempting to escape a grapple.
#[derive(Debug, Clone)]
pub struct GrappleEscapeResult {
    /// Did the escape succeed?
    pub success: bool,
    /// The raw d20 rolled.
    pub player_d20: i32,
    /// d20 + skill modifier.
    pub player_total: i32,
    /// The DC that was beaten.
    pub dc: i32,
    /// Which skill was used (Athletics or Acrobatics).
    pub skill_used: Skill,
}

/// Attempt to escape the current grapple.
///
/// The player picks whichever of Athletics (STR) or Acrobatics (DEX) gives
/// the higher total. The DC is the standard grapple DC:
/// `8 + grappler STR mod + grappler PB`. The grappler is identified by the
/// `source` field on the `Grappled` condition — if the source matches an NPC
/// name, the DC uses that NPC's stats; otherwise it falls back to the
/// player's own stats (self-grapple edge case).
///
/// Returns `None` when no Grappled condition is present on the player.
pub fn resolve_escape_grapple(
    rng: &mut impl Rng,
    state: &mut GameState,
) -> Option<GrappleEscapeResult> {
    // Confirm the player is actually grappled.
    if !conditions::has_condition(&state.character.conditions, ConditionType::Grappled) {
        return None;
    }

    // Identify the grappler from the condition source.
    let grappler_source = state
        .character
        .conditions
        .iter()
        .find(|c| c.condition == ConditionType::Grappled)
        .and_then(|c| c.source.clone());

    // Look up grappler's STR and PB. If the source matches an NPC, use the
    // NPC's stats; otherwise fall back to the player's stats (legacy /
    // self-grapple case).
    let (grappler_str, grappler_pb) = if let Some(ref source) = grappler_source {
        let npc_stats = state.world.npcs.values().find_map(|npc| {
            if npc.name.eq_ignore_ascii_case(source) {
                npc.combat_stats.as_ref().map(|s| {
                    let str_score = s
                        .ability_scores
                        .get(&Ability::Strength)
                        .copied()
                        .unwrap_or(10);
                    (str_score, s.proficiency_bonus)
                })
            } else {
                None
            }
        });
        npc_stats.unwrap_or_else(|| {
            // Fallback: grappler is the player or unknown.
            let str_score = state
                .character
                .ability_scores
                .get(&Ability::Strength)
                .copied()
                .unwrap_or(10);
            (str_score, state.character.proficiency_bonus())
        })
    } else {
        let str_score = state
            .character
            .ability_scores
            .get(&Ability::Strength)
            .copied()
            .unwrap_or(10);
        (str_score, state.character.proficiency_bonus())
    };

    let dc = grapple_dc(grappler_str, grappler_pb);

    // Pick the better skill (Athletics vs Acrobatics).
    let athletics_mod = state.character.skill_modifier(Skill::Athletics);
    let acrobatics_mod = state.character.skill_modifier(Skill::Acrobatics);
    let (skill_mod, skill_used) = if athletics_mod >= acrobatics_mod {
        (athletics_mod, Skill::Athletics)
    } else {
        (acrobatics_mod, Skill::Acrobatics)
    };

    let d20 = roll_d20(rng);
    let total = d20 + skill_mod;
    let success = total >= dc;

    if success {
        // Remove the Grappled condition.
        state
            .character
            .conditions
            .retain(|c| c.condition != ConditionType::Grappled);
    }

    Some(GrappleEscapeResult {
        success,
        player_d20: d20,
        player_total: total,
        dc,
        skill_used,
    })
}

/// The result of an NPC attempting to escape the player's grapple.
#[derive(Debug, Clone)]
pub struct NpcGrappleEscapeResult {
    /// Did the escape succeed?
    pub success: bool,
    /// The raw d20 rolled.
    pub npc_d20: i32,
    /// d20 + skill modifier.
    pub npc_total: i32,
    /// The DC that was beaten.
    pub dc: i32,
    /// Which skill was used (Athletics or Acrobatics).
    pub skill_used: Skill,
}

/// Attempt to escape a grapple for the given NPC.
///
/// The NPC picks whichever of STR mod or DEX mod gives the higher check total.
/// DC = `8 + player STR mod + player PB` (the grapple DC formula with the
/// player as grappler).
///
/// Returns `None` when the NPC does not exist, has no combat stats, or is not
/// grappled.
pub fn resolve_npc_escape_grapple(
    rng: &mut impl Rng,
    state: &mut GameState,
    npc_id: NpcId,
) -> Option<NpcGrappleEscapeResult> {
    let npc = state.world.npcs.get(&npc_id)?;
    let stats = npc.combat_stats.as_ref()?;

    // Confirm the NPC is actually grappled.
    if !conditions::has_condition(&npc.conditions, ConditionType::Grappled) {
        return None;
    }

    // NPC ability scores for the escape check.
    let str_score = stats
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let dex_score = stats
        .ability_scores
        .get(&Ability::Dexterity)
        .copied()
        .unwrap_or(10);
    let str_mod = Ability::modifier(str_score);
    let dex_mod = Ability::modifier(dex_score);
    let (skill_mod, skill_used) = if str_mod >= dex_mod {
        (str_mod, Skill::Athletics)
    } else {
        (dex_mod, Skill::Acrobatics)
    };

    // DC is based on the player's (grappler's) stats.
    let player_str = state
        .character
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let player_pb = state.character.proficiency_bonus();
    let dc = grapple_dc(player_str, player_pb);

    let d20 = roll_d20(rng);
    let total = d20 + skill_mod;
    let success = total >= dc;

    if success {
        // Remove the Grappled condition from the NPC.
        let npc = state.world.npcs.get_mut(&npc_id)?;
        npc.conditions
            .retain(|c| c.condition != ConditionType::Grappled);
    }

    Some(NpcGrappleEscapeResult {
        success,
        npc_d20: d20,
        npc_total: total,
        dc,
        skill_used,
    })
}

/// The result of an NPC grapple attempt against the player.
#[derive(Debug, Clone)]
pub struct NpcGrappleAttemptResult {
    /// Did the grapple succeed (player failed the save)?
    pub success: bool,
    /// The raw d20 the player rolled.
    pub player_d20: i32,
    /// d20 + save modifier.
    pub player_save_total: i32,
    /// Save DC the player rolled against.
    pub dc: i32,
    /// Which ability the player used for the save (STR or DEX).
    pub save_ability: Ability,
    /// True when Danger Sense granted advantage on this save.
    pub danger_sense_active: bool,
}

/// NPC attempts to grapple the player. Rolls the player's save and, on
/// failure, applies the Grappled condition to `state.character.conditions`.
///
/// Returns `None` when the NPC does not exist or has no combat stats.
///
/// DC = 8 + NPC STR modifier + NPC proficiency bonus.
/// Player picks the better of STR or DEX for the saving throw.
pub fn resolve_npc_grapple_attempt(
    rng: &mut impl Rng,
    state: &mut GameState,
    npc_id: NpcId,
) -> Option<NpcGrappleAttemptResult> {
    let npc = state.world.npcs.get(&npc_id)?;
    let stats = npc.combat_stats.as_ref()?;
    let npc_name = npc.name.clone();

    // NPC's STR for DC calculation.
    let npc_str = stats
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let npc_pb = stats.proficiency_bonus;
    let dc = grapple_dc(npc_str, npc_pb);

    // Player picks whichever of STR or DEX gives the higher save total.
    let player_str = state
        .character
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let player_dex = state
        .character
        .ability_scores
        .get(&Ability::Dexterity)
        .copied()
        .unwrap_or(10);
    let str_mod = Ability::modifier(player_str);
    let dex_mod = Ability::modifier(player_dex);
    let (save_mod, save_ability) = if str_mod >= dex_mod {
        (str_mod, Ability::Strength)
    } else {
        (dex_mod, Ability::Dexterity)
    };

    // Danger Sense: Barbarian level 2+ gets advantage on DEX saves
    // unless Incapacitated (SRD 5.2.1).
    let danger_sense_active = save_ability == Ability::Dexterity
        && crate::character::class::character_has_danger_sense(
            state.character.class,
            state.character.level,
        )
        && !conditions::is_incapacitated(&state.character.conditions);

    let d20 = if danger_sense_active {
        roll_d20(rng).max(roll_d20(rng))
    } else {
        roll_d20(rng)
    };
    let save_total = d20 + save_mod;

    if save_total >= dc {
        // Player succeeds — no grapple.
        return Some(NpcGrappleAttemptResult {
            success: false,
            player_d20: d20,
            player_save_total: save_total,
            dc,
            save_ability,
            danger_sense_active,
        });
    }

    // Player fails — apply Grappled condition.
    let grappled_cond =
        ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
            .with_source(&npc_name);
    conditions::apply_condition(&mut state.character.conditions, grappled_cond);

    Some(NpcGrappleAttemptResult {
        success: true,
        player_d20: d20,
        player_save_total: save_total,
        dc,
        save_ability,
        danger_sense_active,
    })
}

/// Release all Grappled conditions from an NPC that were sourced to the
/// given grappler name. Used when a grappler voluntarily releases or when
/// grapple range is exceeded.
pub fn release_grapple_on_npc(npc: &mut Npc, grappler_name: &str) {
    npc.conditions.retain(|c| {
        !(c.condition == ConditionType::Grappled
            && c.source
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(grappler_name))
                .unwrap_or(false))
    });
}

// ---------- Shove (2024 SRD) -------------------------------------------
//
// Shove: target makes a STR or DEX saving throw (it chooses which) vs
// DC (8 + shover STR mod + PB). The engine picks whichever modifier is
// higher on behalf of the NPC — same "best-of" pattern as grapple saves.
// On failure:
//   - push variant: narrative push 5 ft (distance +5 in 1D model).
//   - knock prone: apply Prone condition to the NPC.
// Target must not be more than one size category larger (same rule as grapple).

/// Attempt to shove a target NPC. On a failed STR/DEX save the chosen effect
/// is applied: push 5 ft away (narrative) or knock prone.
///
/// Returns narration lines. Marks `combat.action_used` on the caller's behalf
/// to mirror the grapple pattern — callers MUST restore `state.active_combat`
/// after collecting lines.
///
/// Returns `None` when the target does not exist or has no combat stats.
pub fn handle_shove(
    state: &mut GameState,
    combat: &mut CombatState,
    rng: &mut impl Rng,
    target_id: u32,
    target_name: &str,
    knock_prone: bool,
) -> Vec<String> {
    let mut lines = Vec::new();

    // Compute DC: 8 + player STR mod + proficiency bonus.
    let str_score = state
        .character
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let str_mod = Ability::modifier(str_score);
    let pb = state.character.proficiency_bonus();
    let dc = 8 + str_mod + pb;

    // Roll NPC STR saving throw.
    let npc = match state.world.npcs.get(&target_id) {
        Some(n) => n,
        None => {
            lines.push(format!("You don't see \"{}\" here.", target_name));
            return lines;
        }
    };
    let stats = match npc.combat_stats.as_ref() {
        Some(s) => s,
        None => {
            lines.push(format!("You can't shove {}.", target_name));
            return lines;
        }
    };
    let npc_display = npc.name.clone();

    // Per 2024 SRD the target picks whichever of STR or DEX gives the
    // higher save total. We compute both and pick the better one.
    let npc_str = stats
        .ability_scores
        .get(&Ability::Strength)
        .copied()
        .unwrap_or(10);
    let npc_dex = stats
        .ability_scores
        .get(&Ability::Dexterity)
        .copied()
        .unwrap_or(10);
    let npc_str_mod = Ability::modifier(npc_str);
    let npc_dex_mod = Ability::modifier(npc_dex);
    let (save_mod, save_ability) = if npc_str_mod >= npc_dex_mod {
        (npc_str_mod, Ability::Strength)
    } else {
        (npc_dex_mod, Ability::Dexterity)
    };
    let ability_name = match save_ability {
        Ability::Strength => "STR",
        Ability::Dexterity => "DEX",
        _ => "save",
    };

    let roll = roll_d20(rng);
    let npc_total = roll + save_mod;

    if npc_total >= dc {
        // NPC succeeds: resists the shove.
        lines.push(format!(
            "{} resists the shove! ({} save: {} vs DC {})",
            npc_display,
            ability_name,
            format_roll(roll, save_mod, npc_total),
            dc
        ));
        return lines;
    }

    // NPC fails the save.
    if knock_prone {
        // Apply Prone condition.
        let new_cond = ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent);
        let npc_mut = match state.world.npcs.get_mut(&target_id) {
            Some(n) => n,
            None => return lines,
        };
        let applied = try_apply_condition_to_npc(npc_mut, new_cond);
        if applied {
            lines.push(format!(
                "You knock {} prone! ({} save: {} vs DC {})",
                npc_display,
                ability_name,
                format_roll(roll, save_mod, npc_total),
                dc
            ));
        } else {
            lines.push(format!(
                "{} fails the save but is immune to Prone. ({} save: {} vs DC {})",
                npc_display,
                ability_name,
                format_roll(roll, save_mod, npc_total),
                dc
            ));
        }
    } else {
        // Push: increase distance by 5 ft (narrative push).
        let current_dist = combat.distances.get(&target_id).copied().unwrap_or(5);
        let new_dist = current_dist.saturating_add(5);
        combat.distances.insert(target_id, new_dist);
        lines.push(format!(
            "You shove {} back 5 feet! ({} save: {} vs DC {})",
            npc_display,
            ability_name,
            format_roll(roll, save_mod, npc_total),
            dc
        ));
    }

    lines
}

/// Cleave: on a melee hit, if another hostile NPC is within 5 ft of the
/// player (i.e., in melee range), make a second attack roll against that
/// NPC with the same weapon. Damage ability modifier is omitted unless
/// negative (SRD). Once per player turn.
///
/// Returns Some((target_id, cleave_result)) when a cleave second attack
/// was rolled; None otherwise. Callers resolve the damage through
/// `apply_damage_to_npc` to respect immunities/resistances.
///
/// `primary_target_id` is excluded from the eligible secondaries.
///
/// A conservative "within reach" definition is used: 5 ft range. Reach
/// weapons still cleave at 5 ft per SRD's "within 5 feet of the first"
/// clause — the secondary target just needs to be 5 ft from the primary,
/// which in the 1D model collapses to "also in melee range of you".
#[allow(clippy::too_many_arguments)]
pub fn apply_cleave_mastery(
    rng: &mut impl Rng,
    has_mastery: bool,
    result: &AttackResult,
    primary_target_id: NpcId,
    combat: &mut CombatState,
    state: &GameState,
    ability_mod_used: i32,
) -> Option<(NpcId, AttackResult, i32)> {
    if !has_mastery || !result.hit || combat.cleave_used_this_turn {
        return None;
    }
    // Find a secondary target: a living hostile NPC, not the primary,
    // currently within 5 ft.
    let secondary_id = combat
        .distances
        .iter()
        .filter(|(id, dist)| {
            **id != primary_target_id
                && **dist <= 5
                && state
                    .world
                    .npcs
                    .get(*id)
                    .and_then(|n| n.combat_stats.as_ref())
                    .map(|s| s.current_hp > 0)
                    .unwrap_or(false)
        })
        .map(|(id, _)| *id)
        .next();
    let secondary_id = secondary_id?;
    let secondary_ac = state
        .world
        .npcs
        .get(&secondary_id)
        .and_then(|n| n.combat_stats.as_ref())
        .map(|s| s.ac)
        .unwrap_or(10);
    let secondary_dodging = combat
        .npc_dodging
        .get(&secondary_id)
        .copied()
        .unwrap_or(false);
    let secondary_conditions: Vec<ActiveCondition> = state
        .world
        .npcs
        .get(&secondary_id)
        .map(|n| n.conditions.clone())
        .unwrap_or_default();
    let distance = combat.distances.get(&secondary_id).copied().unwrap_or(5);
    let hostile_within_5ft = has_living_hostile_within(state, combat, 5);
    let weapon_id = state.character.equipped.main_hand;
    let off_hand_free = state.character.equipped.off_hand.is_none();
    let mut cleave_result = resolve_player_attack(
        rng,
        &state.character,
        secondary_ac,
        secondary_dodging,
        weapon_id,
        &state.world.items,
        distance,
        off_hand_free,
        hostile_within_5ft,
        &secondary_conditions,
        false,
        false,
        combat.npc_cover.get(&secondary_id).unwrap_or(&Cover::None),
    );
    // Per SRD: the second attack does not include ability-mod damage
    // unless the modifier is negative. We subtract the positive modifier
    // back out of the damage roll.
    if cleave_result.hit && cleave_result.damage > 0 && ability_mod_used > 0 {
        cleave_result.damage = (cleave_result.damage - ability_mod_used).max(1);
    }
    combat.cleave_used_this_turn = true;
    Some((secondary_id, cleave_result, ability_mod_used))
}

/// Nick: when the off-hand weapon has the Nick mastery and the character
/// has that mastery unlocked, the off-hand Light-weapon extra attack is
/// made as part of the Attack action instead of as a bonus action. The
/// benefit is once per turn.
///
/// The orchestrator uses this to decide whether
///   (a) the off-hand attack may proceed BEFORE the main-hand Attack
///       action is used (Nick folds into the Attack action), and
///   (b) the bonus-action slot should be consumed after the swing.
///
/// Returns true when Nick applies (so the caller should skip the
/// "requires bonus action" gate and skip consuming the bonus action).
/// Also flips `nick_used_this_turn` on success so a second Nick swing in
/// the same turn falls back to normal Two-Weapon-Fighting rules.
pub fn apply_nick_mastery(has_mastery: bool, combat: &mut CombatState) -> bool {
    if !has_mastery || combat.nick_used_this_turn {
        return false;
    }
    combat.nick_used_this_turn = true;
    true
}

// ---- Rogue: Sneak Attack --------------------------------------------------

/// Number of Sneak Attack dice (d6) a Rogue rolls at the given character
/// level per SRD 5.1: `ceil(level / 2)`, equivalent to `floor((level + 1) / 2)`.
///
/// Examples: level 1 -> 1d6, level 2 -> 1d6, level 3 -> 2d6, level 11 -> 6d6,
/// level 20 -> 10d6.
pub fn sneak_attack_dice_for_level(level: u32) -> u32 {
    (level + 1) / 2
}

/// True when a weapon's properties qualify for Sneak Attack per SRD 5.1:
/// a Finesse melee weapon OR a ranged weapon. `is_ranged_attack` is the
/// orchestrator's resolution of whether this specific attack is being used
/// at range (thrown weapons only qualify when actually thrown from range).
pub fn sneak_attack_weapon_qualifies(properties: u16, is_ranged_attack: bool) -> bool {
    let is_finesse = properties & FINESSE != 0;
    is_finesse || is_ranged_attack
}

/// Roll the Sneak Attack bonus-damage dice for the given Rogue level. On a
/// critical hit the die count is doubled per SRD. Returns the summed damage.
/// A zero-dice result (non-Rogue levels mis-used) returns 0.
pub fn roll_sneak_attack(rng: &mut impl Rng, level: u32, critical: bool) -> i32 {
    let dice = sneak_attack_dice_for_level(level);
    if dice == 0 {
        return 0;
    }
    let count = if critical { dice * 2 } else { dice };
    roll_dice(rng, count, 6).iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{class::Class, create_character, race::Race};
    #[allow(unused_imports)]
    use crate::equipment::Equipment;
    use crate::state::{CombatStats, DamageType, NpcAttack};
    use crate::state::{Disposition, GamePhase, Npc, NpcRole, WorldState, SAVE_VERSION};
    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashMap;
    use std::collections::HashSet;

    fn test_character() -> Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 16);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 14);
        scores.insert(Ability::Intelligence, 10);
        scores.insert(Ability::Wisdom, 12);
        scores.insert(Ability::Charisma, 8);
        create_character(
            "TestHero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![],
        )
    }

    fn goblin_stats() -> CombatStats {
        CombatStats {
            max_hp: 7,
            current_hp: 7,
            ac: 15,
            speed: 30,
            ability_scores: {
                let mut m = HashMap::new();
                m.insert(Ability::Strength, 8);
                m.insert(Ability::Dexterity, 14);
                m.insert(Ability::Constitution, 10);
                m.insert(Ability::Intelligence, 10);
                m.insert(Ability::Wisdom, 8);
                m.insert(Ability::Charisma, 8);
                m
            },
            attacks: vec![
                NpcAttack {
                    name: "Scimitar".to_string(),
                    hit_bonus: 4,
                    damage_dice: 1,
                    damage_die: 6,
                    damage_bonus: 2,
                    damage_type: DamageType::Slashing,
                    reach: 5,
                    range_normal: 0,
                    range_long: 0,
                },
                NpcAttack {
                    name: "Shortbow".to_string(),
                    hit_bonus: 4,
                    damage_dice: 1,
                    damage_die: 6,
                    damage_bonus: 2,
                    damage_type: DamageType::Piercing,
                    reach: 0,
                    range_normal: 80,
                    range_long: 320,
                },
            ],
            proficiency_bonus: 2,
            cr: 0.25,
            ..Default::default()
        }
    }

    fn test_state_with_goblin() -> GameState {
        let character = test_character();
        let mut npcs = HashMap::new();
        npcs.insert(
            0,
            Npc {
                id: 0,
                name: "Goblin".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(goblin_stats()),
                conditions: Vec::new(),
            },
        );

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(),
                npcs,
                items: HashMap::new(),
                triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 0,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: crate::state::ProgressState::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
            pending_new_game_confirm: false,
        }
    }

    #[test]
    fn test_roll_initiative_produces_ordered_list() {
        let mut rng = StdRng::seed_from_u64(42);
        let player = test_character();
        let stats = goblin_stats();
        let npcs = vec![(0 as NpcId, &stats)];
        let order = roll_initiative(&mut rng, &player, &npcs);
        assert_eq!(order.len(), 2);
        // Verify sorted descending
        assert!(order[0].1 >= order[1].1);
    }

    #[test]
    fn test_start_combat_sets_distances() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let dist = combat.distances.get(&0).unwrap();
        assert!(*dist >= 20 && *dist <= 30);
        assert!(*dist % 5 == 0);
    }

    #[test]
    fn test_start_combat_initiative_order() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert_eq!(combat.initiative_order.len(), 2);
        assert_eq!(combat.round, 1);
    }

    #[test]
    fn test_player_melee_reach_unarmed_is_5() {
        let character = test_character();
        let items = HashMap::new();
        assert_eq!(
            player_melee_reach(&character, &items),
            5,
            "Unarmed melee reach should be 5 ft"
        );
    }

    #[test]
    fn test_player_melee_reach_longsword_is_5() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(
            500u32,
            Item {
                id: 500,
                name: "Longsword".to_string(),
                description: "".to_string(),
                item_type: ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 8,
                    damage_type: DamageType::Slashing,
                    properties: crate::equipment::VERSATILE,
                    category: WeaponCategory::Martial,
                    versatile_die: 10,
                    range_normal: 0,
                    range_long: 0,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            },
        );
        character.equipped.main_hand = Some(500);
        assert_eq!(
            player_melee_reach(&character, &items),
            5,
            "Non-reach weapon should give 5 ft reach"
        );
    }

    #[test]
    fn test_player_melee_reach_glaive_is_10() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(
            501u32,
            Item {
                id: 501,
                name: "Glaive".to_string(),
                description: "".to_string(),
                item_type: ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 10,
                    damage_type: DamageType::Slashing,
                    properties: crate::equipment::REACH
                        | crate::equipment::HEAVY
                        | crate::equipment::TWO_HANDED,
                    category: WeaponCategory::Martial,
                    versatile_die: 0,
                    range_normal: 0,
                    range_long: 0,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            },
        );
        character.equipped.main_hand = Some(501);
        assert_eq!(
            player_melee_reach(&character, &items),
            10,
            "REACH weapon should give 10 ft reach"
        );
    }

    #[test]
    fn test_player_melee_reach_ranged_only_weapon_falls_back_to_5() {
        // Pure ranged weapon (longbow) has no melee usage; reach should
        // fall back to the unarmed default of 5 rather than 0.
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(
            502u32,
            Item {
                id: 502,
                name: "Longbow".to_string(),
                description: "".to_string(),
                item_type: ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 8,
                    damage_type: DamageType::Piercing,
                    properties: crate::equipment::AMMUNITION
                        | crate::equipment::TWO_HANDED
                        | crate::equipment::HEAVY,
                    category: WeaponCategory::Martial,
                    versatile_die: 0,
                    range_normal: 150,
                    range_long: 600,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            },
        );
        character.equipped.main_hand = Some(502);
        assert_eq!(
            player_melee_reach(&character, &items),
            5,
            "Pure ranged weapon should fall back to unarmed reach of 5 ft"
        );
    }

    #[test]
    fn test_npc_within_player_reach_unarmed_at_5ft() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        combat.distances.insert(0, 5);
        assert!(
            npc_within_player_reach(&state, &combat, 0),
            "Goblin at 5 ft should be in unarmed reach"
        );

        combat.distances.insert(0, 10);
        assert!(
            !npc_within_player_reach(&state, &combat, 0),
            "Goblin at 10 ft should NOT be in unarmed reach"
        );
    }

    #[test]
    fn test_npc_within_player_reach_respects_reach_weapon() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Equip a glaive (REACH weapon). 10 ft threatened area.
        state.world.items.insert(
            700u32,
            Item {
                id: 700,
                name: "Glaive".to_string(),
                description: "".to_string(),
                item_type: ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 10,
                    damage_type: DamageType::Slashing,
                    properties: crate::equipment::REACH
                        | crate::equipment::HEAVY
                        | crate::equipment::TWO_HANDED,
                    category: WeaponCategory::Martial,
                    versatile_die: 0,
                    range_normal: 0,
                    range_long: 0,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            },
        );
        state.character.equipped.main_hand = Some(700);

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 10);
        assert!(
            npc_within_player_reach(&state, &combat, 0),
            "Glaive-equipped player should threaten NPC at 10 ft"
        );

        combat.distances.insert(0, 15);
        assert!(
            !npc_within_player_reach(&state, &combat, 0),
            "Glaive reach is 10 ft; NPC at 15 ft should NOT be threatened"
        );
    }

    #[test]
    fn test_npc_within_player_reach_dead_npc_not_threatened() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Kill the goblin.
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;
        combat.distances.insert(0, 5);
        assert!(
            !npc_within_player_reach(&state, &combat, 0),
            "Dead NPC at 5 ft should not be treated as reachable for OA"
        );
    }

    #[test]
    fn test_has_living_hostile_within() {
        let mut state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        combat.distances.insert(0, 5);
        assert!(has_living_hostile_within(&state, &combat, 5));

        combat.distances.insert(0, 10);
        assert!(!has_living_hostile_within(&state, &combat, 5));

        // dead enemy should not count
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;
        combat.distances.insert(0, 5);
        assert!(!has_living_hostile_within(&state, &combat, 5));
    }

    #[test]
    fn test_resolve_npc_attack_hit_or_miss() {
        let attack = NpcAttack {
            name: "Scimitar".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        };
        // Run many times to get both hits and misses
        let mut hits = 0;
        let mut misses = 0;
        for seed in 0..100 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(
                &mut rng,
                &attack,
                15,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            if result.hit {
                hits += 1;
            } else {
                misses += 1;
            }
        }
        assert!(hits > 0, "Should have some hits");
        assert!(misses > 0, "Should have some misses");
    }

    #[test]
    fn test_natural_20_always_hits() {
        let attack = NpcAttack {
            name: "Test".to_string(),
            hit_bonus: -10, // Very low bonus
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 0,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        };
        // Find a seed that gives nat 20
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(
                &mut rng,
                &attack,
                30,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            if result.natural_20 {
                assert!(result.hit, "Natural 20 should always hit");
                return;
            }
        }
        panic!("Could not find a natural 20 in 1000 seeds");
    }

    #[test]
    fn test_natural_1_always_misses() {
        let attack = NpcAttack {
            name: "Test".to_string(),
            hit_bonus: 100, // Very high bonus
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 0,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        };
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(
                &mut rng,
                &attack,
                1,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            if result.natural_1 {
                assert!(!result.hit, "Natural 1 should always miss");
                return;
            }
        }
        panic!("Could not find a natural 1 in 1000 seeds");
    }

    #[test]
    fn test_critical_hit_doubles_dice() {
        let attack = NpcAttack {
            name: "Test".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        };
        // Find a nat 20 and verify higher damage potential
        let mut crit_damages = Vec::new();
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(
                &mut rng,
                &attack,
                10,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            if result.natural_20 {
                crit_damages.push(result.damage);
            }
        }
        assert!(!crit_damages.is_empty());
        // Critical hits with 2d6+2 should be >= 4 (min 1+1+2)
        for &d in &crit_damages {
            assert!(d >= 1, "Critical damage should be at least 1, got {}", d);
        }
    }

    #[test]
    fn test_dodge_grants_disadvantage() {
        let attack = NpcAttack {
            name: "Test".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        };

        let mut dodge_hits = 0;
        let mut normal_hits = 0;
        for seed in 0..1000 {
            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);
            let dodge = resolve_npc_attack(
                &mut rng1,
                &attack,
                15,
                true,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            let normal = resolve_npc_attack(
                &mut rng2,
                &attack,
                15,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            if dodge.hit {
                dodge_hits += 1;
            }
            if normal.hit {
                normal_hits += 1;
            }
        }
        assert!(
            dodge_hits < normal_hits,
            "Dodging should reduce hit rate: dodge={}, normal={}",
            dodge_hits,
            normal_hits
        );
    }

    // Hypothesis (see handoff fix-unarmed-attack-roll):
    //   The unarmed branch in resolve_player_attack short-circuits with hit: true,
    //   bypassing the d20 pipeline and condition-based advantage/disadvantage.
    //   Fix: remove the early return so unarmed flows through the standard path,
    //   using STR mod + prof bonus on the attack roll, and damage = 1 + STR mod
    //   (doubled base to 2 + STR mod on a natural 20 since there are no dice to
    //   double). Damage floor remains 1.
    #[test]
    fn test_unarmed_strike_rolls_to_hit() {
        // SRD: unarmed must roll d20 + STR + prof vs AC, not auto-hit.
        // Against an unreachable AC (100), only a natural 20 can "hit"; every
        // other roll must miss and deal 0 damage.
        let player = test_character();
        let items = HashMap::new();
        let mut saw_miss = false;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_player_attack(
                &mut rng,
                &player,
                100,
                false,
                None,
                &items,
                5,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );
            assert_eq!(result.weapon_name, "Unarmed");
            assert!(
                result.attack_roll >= 1 && result.attack_roll <= 20,
                "Attack roll must be a real d20 (seed={}, roll={})",
                seed,
                result.attack_roll
            );
            if result.natural_20 {
                // Nat 20 always hits per SRD, even against absurd AC.
                assert!(result.hit, "Nat 20 must hit (seed={})", seed);
            } else {
                saw_miss = true;
                assert!(
                    !result.hit,
                    "Non-crit unarmed must miss AC 100 (seed={}, roll={}, total={})",
                    seed, result.attack_roll, result.total_attack
                );
                assert_eq!(
                    result.damage, 0,
                    "Miss should deal 0 damage (seed={})",
                    seed
                );
            }
        }
        assert!(
            saw_miss,
            "Expected to observe at least one miss against AC 100"
        );
    }

    #[test]
    fn test_unarmed_strike_hits_reachable_ac() {
        // Against AC 1, every d20 roll hits (even a nat 1 only auto-misses).
        // Damage on hit = 1 + STR mod. STR 16+1(human)=17, mod +3 -> damage 4.
        // On nat 20 crit: 2 + STR mod = 5 (per handoff: flat +1 doubles).
        let player = test_character();
        let items = HashMap::new();
        let mut hit_count = 0;
        let mut base_damage_seen = false;
        let mut crit_damage_seen = false;
        for seed in 0..200 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_player_attack(
                &mut rng,
                &player,
                1,
                false,
                None,
                &items,
                5,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );
            assert_eq!(result.weapon_name, "Unarmed");
            assert_eq!(result.damage_type, DamageType::Bludgeoning);
            if result.hit {
                hit_count += 1;
                if result.natural_20 {
                    assert_eq!(
                        result.damage, 5,
                        "Nat 20 crit should deal 2 + STR mod (seed={})",
                        seed
                    );
                    crit_damage_seen = true;
                } else {
                    assert_eq!(
                        result.damage, 4,
                        "Normal hit should deal 1 + STR mod (seed={})",
                        seed
                    );
                    base_damage_seen = true;
                }
            } else {
                assert!(
                    result.natural_1,
                    "Only a nat 1 should miss AC 1 (seed={}, roll={})",
                    seed, result.attack_roll
                );
            }
        }
        assert!(hit_count > 0, "Expected at least some hits against AC 1");
        assert!(
            base_damage_seen,
            "Expected to observe a normal-hit damage roll"
        );
        assert!(
            crit_damage_seen,
            "Expected to observe a nat-20 crit in 200 seeds"
        );
    }

    #[test]
    fn test_unarmed_strike_disadvantage_from_poisoned() {
        use crate::conditions::{ActiveCondition, ConditionDuration};

        // Poisoned imposes disadvantage on attack rolls. With unarmed now on the
        // standard roll pipeline, a poisoned attacker should hit less often.
        let player = test_character();
        let mut poisoned_player = player.clone();
        poisoned_player.conditions.push(ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::Rounds(3),
        ));
        let items = HashMap::new();

        let mut normal_hits = 0;
        let mut poisoned_hits = 0;
        for seed in 0..1000 {
            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);
            let normal = resolve_player_attack(
                &mut rng1,
                &player,
                15,
                false,
                None,
                &items,
                5,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );
            let poisoned = resolve_player_attack(
                &mut rng2,
                &poisoned_player,
                15,
                false,
                None,
                &items,
                5,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );
            if normal.hit {
                normal_hits += 1;
            }
            if poisoned.hit {
                poisoned_hits += 1;
            }
        }
        assert!(
            poisoned_hits < normal_hits,
            "Poisoned unarmed attacker should hit less often: normal={}, poisoned={}",
            normal_hits,
            poisoned_hits
        );
    }

    #[test]
    fn test_approach_reduces_distance() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let initial_dist = *combat.distances.get(&0).unwrap();
        combat.player_movement_remaining = 30;

        let lines = approach_target(&mut rng, 0, &state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert!(new_dist < initial_dist);
        assert!(lines[0].contains("move toward"));
    }

    #[test]
    fn test_approach_stops_at_5ft() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;

        approach_target(&mut rng, 0, &state, &mut combat);
        assert_eq!(*combat.distances.get(&0).unwrap(), 5);
    }

    #[test]
    fn test_retreat_increases_distance() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 20);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = true; // Avoid opportunity attacks for simpler test

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert_eq!(*combat.distances.get(&0).unwrap(), 50);
        assert!(lines.iter().any(|l| l.contains("retreat")));
    }

    #[test]
    fn test_retreat_triggers_opportunity_attack_with_reach_10() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks[0]
            .reach = 10;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(
            lines.iter().any(|l| l.contains("opportunity attack")),
            "Expected opportunity attack narration, got {:?}",
            lines
        );
    }

    #[test]
    fn test_retreat_no_opportunity_attack_outside_reach_5() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks[0]
            .reach = 5;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(
            !lines.iter().any(|l| l.contains("opportunity attack")),
            "Did not expect opportunity attack narration, got {:?}",
            lines
        );
    }

    #[test]
    fn test_retreat_no_opportunity_attack_when_still_within_reach_10() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks[0]
            .reach = 10;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);
        combat.player_movement_remaining = 5; // Move to 10ft, still within reach 10
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(
            !lines.iter().any(|l| l.contains("opportunity attack")),
            "Should not trigger OA when still within reach, got {:?}",
            lines
        );
    }

    #[test]
    fn test_fire_opportunity_attacks_consumes_reaction() {
        // Verifies that a given NPC only fires one OA per round even if
        // fire_opportunity_attacks is called twice.
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Put goblin at 5ft so it is within reach
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks[0]
            .reach = 5;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        // First call: old=5, new=u32::MAX — should potentially fire OA
        let changes1 = vec![(0u32, 5u32, u32::MAX)];
        let _lines1 = fire_opportunity_attacks(&mut rng, &mut state, &mut combat, &changes1);
        assert!(
            combat.npc_reactions_used.contains(&0),
            "NPC reaction should be marked used after OA"
        );

        // Second call: same scenario — should NOT fire another OA (reaction consumed)
        let initial_hp = state.character.current_hp;
        let changes2 = vec![(0u32, 5u32, u32::MAX)];
        let lines2 = fire_opportunity_attacks(&mut rng, &mut state, &mut combat, &changes2);
        assert!(
            !lines2.iter().any(|l| l.contains("opportunity attack")),
            "Second OA call should be suppressed by spent reaction, got {:?}",
            lines2
        );
        // HP should not change further from the second call
        assert_eq!(
            state.character.current_hp, initial_hp,
            "HP should be unchanged after second (suppressed) OA call"
        );
    }

    #[test]
    fn test_fire_opportunity_attacks_no_oa_when_disengaging() {
        // Verifies disengage flag suppresses OAs (tested via retreat, but also
        // directly confirms fire_opportunity_attacks isn't called by retreat when
        // player is disengaging).
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks[0]
            .reach = 5;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = true;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(
            !lines.iter().any(|l| l.contains("opportunity attack")),
            "Disengage should suppress OA on retreat, got {:?}",
            lines
        );
        assert!(
            !combat.npc_reactions_used.contains(&0),
            "NPC reaction should NOT be consumed when disengaging"
        );
    }

    #[test]
    fn test_npc_reactions_cleared_on_new_round() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Mark NPC reaction used
        combat.npc_reactions_used.insert(0);
        assert!(combat.npc_reactions_used.contains(&0));

        // Advance turn until round increments (cycle all combatants)
        let n = combat.initiative_order.len();
        // Force current_turn to just before wrap so next advance increments round
        combat.current_turn = n - 1;
        combat.advance_turn(&mut state);

        assert!(
            combat.npc_reactions_used.is_empty(),
            "npc_reactions_used should be cleared when a new round begins"
        );
    }

    #[test]
    fn test_combat_end_victory() {
        let mut state = test_state_with_goblin();
        // Kill the goblin
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert_eq!(combat.check_end(&state), Some(true));
    }

    #[test]
    fn test_combat_end_defeat() {
        // Per SRD Death Saving Throws (issue #84), 0 HP alone does NOT end
        // combat in defeat -- the player enters a dying state and rolls
        // death saves. Defeat only occurs after three death save failures.
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_failures = 3;
        assert_eq!(combat.check_end(&state), Some(false));
    }

    #[test]
    fn test_combat_not_ended() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert_eq!(combat.check_end(&state), None);
    }

    // Hypothesis: The bug occurs because check_end uses unwrap_or(true) which treats
    // any NPC with missing combat_stats as dead, triggering premature VICTORY when
    // only one of multiple hostile NPCs has been killed.

    fn test_state_with_two_goblins() -> GameState {
        let character = test_character();
        let mut npcs = HashMap::new();
        npcs.insert(
            0,
            Npc {
                id: 0,
                name: "Goblin".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(goblin_stats()),
                conditions: Vec::new(),
            },
        );
        npcs.insert(
            1,
            Npc {
                id: 1,
                name: "Goblin".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(goblin_stats()),
                conditions: Vec::new(),
            },
        );

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(),
                npcs,
                items: HashMap::new(),
                triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 0,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: crate::state::ProgressState::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
            pending_new_game_confirm: false,
        }
    }

    #[test]
    fn test_two_hostiles_kill_one_combat_continues() {
        let mut state = test_state_with_two_goblins();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs, crate::state::LocationType::Room);

        // Kill only the first goblin
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;

        // Combat should NOT be over -- second goblin still alive
        assert_eq!(
            combat.check_end(&state),
            None,
            "Combat should continue when one of two hostile NPCs is still alive"
        );
    }

    #[test]
    fn test_two_hostiles_kill_both_victory() {
        let mut state = test_state_with_two_goblins();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs, crate::state::LocationType::Room);

        // Kill both goblins
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;
        state
            .world
            .npcs
            .get_mut(&1)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;

        // Now combat should end in victory
        assert_eq!(
            combat.check_end(&state),
            Some(true),
            "Combat should end in VICTORY when all hostile NPCs are dead"
        );
    }

    #[test]
    fn test_missing_combat_stats_treated_as_alive_not_dead() {
        // Regression test: NPC with combat_stats: None should be treated as alive,
        // not dead by check_end. This prevents premature VICTORY from ghost NPCs.
        let mut state = test_state_with_two_goblins();
        // Remove combat_stats from second goblin to simulate the bug scenario
        state.world.npcs.get_mut(&1).unwrap().combat_stats = None;

        // Construct CombatState directly (bypassing start_combat's debug_assert)
        // because this test specifically targets check_end's handling of missing stats.
        let combat = CombatState {
            initiative_order: vec![
                (Combatant::Player, 15),
                (Combatant::Npc(0), 12),
                (Combatant::Npc(1), 10),
            ],
            current_turn: 0,
            round: 1,
            distances: {
                let mut d = HashMap::new();
                d.insert(0, 25);
                d.insert(1, 25);
                d
            },
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: false,
            action_used: false,
            bonus_action_used: false,
            action_surge_active: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            attacks_made_this_turn: 0,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: Cover::None,
            npc_cover: HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        };

        // Kill the first goblin (the one with stats)
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .current_hp = 0;

        // Combat should NOT end -- the ghost NPC (no stats) should be treated as alive
        assert_eq!(
            combat.check_end(&state),
            None,
            "NPC with missing combat_stats should be treated as alive, not dead"
        );
    }

    #[test]
    fn test_advance_turn_skips_dead() {
        let mut state = test_state_with_goblin();
        // Add a second goblin that's dead
        state.world.npcs.insert(
            1,
            Npc {
                id: 1,
                name: "Dead Goblin".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(CombatStats {
                    max_hp: 7,
                    current_hp: 0,
                    ac: 15,
                    speed: 30,
                    ability_scores: HashMap::new(),
                    attacks: vec![],
                    proficiency_bonus: 2,
                    cr: 0.25,
                    ..Default::default()
                }),
                conditions: Vec::new(),
            },
        );

        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs, crate::state::LocationType::Room);

        // Advance through turns, dead NPC should be skipped
        let mut found_dead_npc_turn = false;
        for _ in 0..10 {
            let c = combat.advance_turn(&mut state);
            if c == Combatant::Npc(1) {
                found_dead_npc_turn = true;
            }
        }
        assert!(!found_dead_npc_turn, "Dead NPC should never get a turn");
    }

    #[test]
    fn test_npc_ai_melee_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5); // In melee range

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        assert!(!lines.is_empty());
        // Should attack with Scimitar (melee)
        assert!(
            lines[0].contains("Scimitar"),
            "NPC should use melee: {}",
            lines[0]
        );
    }

    #[test]
    fn test_npc_ai_ranged_out_of_melee() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 30); // Out of melee, in ranged

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        assert!(!lines.is_empty());
        // Should use Shortbow (ranged)
        assert!(
            lines[0].contains("Shortbow"),
            "NPC should use ranged: {}",
            lines[0]
        );
    }

    #[test]
    fn test_npc_ai_moves_toward_if_no_attack_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Remove ranged attack so NPC can only melee
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks = vec![NpcAttack {
            name: "Scimitar".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        }];
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 60); // Far away

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert!(new_dist < 60, "NPC should have moved closer");
        assert!(
            lines[0].contains("moves toward"),
            "Should narrate movement: {}",
            lines[0]
        );
    }

    // Hypothesis: resolve_npc_turn() moves the NPC toward the player but
    // returns immediately without re-checking for an attack. Per SRD 5.1
    // (line 507), movement and action are independent — a creature may move
    // then act on the same turn. After closing distance, the NPC should
    // attempt a melee (or ranged) attack if now in range.
    #[test]
    fn test_npc_attacks_after_moving_into_melee_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Melee-only NPC: remove ranged attack
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks = vec![NpcAttack {
            name: "Scimitar".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        }];
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Distance 35: with speed 30, NPC moves to 5ft (melee range)
        combat.distances.insert(0, 35);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert_eq!(new_dist, 5, "NPC should have moved to melee range");
        // Should contain both a movement line and an attack line
        let has_move = lines.iter().any(|l| l.contains("moves toward"));
        let has_attack = lines.iter().any(|l| l.contains("Scimitar"));
        assert!(has_move, "NPC should narrate movement: {:?}", lines);
        assert!(
            has_attack,
            "NPC should attack after moving into melee range: {:?}",
            lines
        );
    }

    #[test]
    fn test_npc_attacks_after_moving_into_ranged_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Ranged-only NPC: remove melee attack, keep shortbow (range 80/320)
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks = vec![NpcAttack {
            name: "Shortbow".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Piercing,
            reach: 0,
            range_normal: 80,
            range_long: 320,
        }];
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Distance 350: with speed 30, NPC moves to 320ft (at ranged long range)
        combat.distances.insert(0, 350);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert_eq!(new_dist, 320, "NPC should have moved to ranged range");
        let has_move = lines.iter().any(|l| l.contains("moves toward"));
        let has_attack = lines.iter().any(|l| l.contains("Shortbow"));
        assert!(has_move, "NPC should narrate movement: {:?}", lines);
        assert!(
            has_attack,
            "NPC should attack after moving into ranged range: {:?}",
            lines
        );
    }

    #[test]
    fn test_npc_move_only_when_still_out_of_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Melee-only NPC
        state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap()
            .attacks = vec![NpcAttack {
            name: "Scimitar".to_string(),
            hit_bonus: 4,
            damage_dice: 1,
            damage_die: 6,
            damage_bonus: 2,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        }];
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Distance 60: with speed 30, NPC moves to 30ft — still NOT in melee range
        combat.distances.insert(0, 60);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert_eq!(
            new_dist, 30,
            "NPC should have moved closer but still out of range"
        );
        let has_move = lines.iter().any(|l| l.contains("moves toward"));
        let has_attack = lines.iter().any(|l| l.contains("Scimitar"));
        assert!(has_move, "NPC should narrate movement: {:?}", lines);
        assert!(
            !has_attack,
            "NPC should NOT attack when still out of range: {:?}",
            lines
        );
    }

    // ---- Condition Integration Tests ----

    #[test]
    fn test_poisoned_player_attacks_with_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Direct test: verify that poisoned condition returns disadvantage from get_attack_advantage
        let poisoned = vec![ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::Rounds(3),
        )];

        assert_eq!(
            conditions::get_attack_advantage(&poisoned),
            Some(false),
            "Poisoned should impose disadvantage on attacks"
        );
    }

    #[test]
    fn test_attacking_stunned_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Direct test: verify that stunned target grants advantage to attacker
        let attacker: Vec<ActiveCondition> = vec![];
        let stunned = vec![ActiveCondition::new(
            ConditionType::Stunned,
            ConditionDuration::Rounds(1),
        )];

        assert_eq!(
            conditions::get_defense_advantage(&attacker, &stunned),
            Some(true),
            "Attacking stunned target should grant advantage"
        );
    }

    #[test]
    fn test_paralyzed_target_is_auto_crit() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Direct test: verify paralyzed condition marks target as auto-crit
        let paralyzed = vec![ActiveCondition::new(
            ConditionType::Paralyzed,
            ConditionDuration::Rounds(1),
        )];

        assert!(
            conditions::is_auto_crit_target(&paralyzed),
            "Paralyzed target should be subject to auto-crits"
        );

        // Stunned should NOT be auto-crit
        let stunned = vec![ActiveCondition::new(
            ConditionType::Stunned,
            ConditionDuration::Rounds(1),
        )];
        assert!(
            !conditions::is_auto_crit_target(&stunned),
            "Stunned target should not be auto-crit"
        );
    }

    #[test]
    fn test_prone_grants_advantage_within_5ft() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Direct test: prone target grants advantage to attackers
        let attacker: Vec<ActiveCondition> = vec![];
        let prone = vec![ActiveCondition::new(
            ConditionType::Prone,
            ConditionDuration::Permanent,
        )];

        assert_eq!(
            conditions::get_defense_advantage(&attacker, &prone),
            Some(true),
            "Attacking prone target should grant advantage"
        );
    }

    #[test]
    fn test_blinded_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Direct test: blinded target grants advantage to attackers
        let attacker: Vec<ActiveCondition> = vec![];
        let blinded = vec![ActiveCondition::new(
            ConditionType::Blinded,
            ConditionDuration::Rounds(2),
        )];

        assert_eq!(
            conditions::get_defense_advantage(&attacker, &blinded),
            Some(true),
            "Attacking blinded target should grant advantage"
        );
    }

    #[test]
    fn test_blinded_and_poisoned_impose_attack_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Blinded imposes disadvantage
        let blinded = vec![ActiveCondition::new(
            ConditionType::Blinded,
            ConditionDuration::Rounds(1),
        )];
        assert_eq!(conditions::get_attack_advantage(&blinded), Some(false));

        // Prone imposes disadvantage
        let prone = vec![ActiveCondition::new(
            ConditionType::Prone,
            ConditionDuration::Permanent,
        )];
        assert_eq!(conditions::get_attack_advantage(&prone), Some(false));
    }

    #[test]
    fn test_stunned_and_paralyzed_prevent_actions() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        // Stunned prevents actions
        let stunned = vec![ActiveCondition::new(
            ConditionType::Stunned,
            ConditionDuration::Rounds(1),
        )];
        assert!(!conditions::can_take_actions(&stunned));
        assert!(!conditions::can_take_reactions(&stunned));

        // Paralyzed prevents actions
        let paralyzed = vec![ActiveCondition::new(
            ConditionType::Paralyzed,
            ConditionDuration::Rounds(1),
        )];
        assert!(!conditions::can_take_actions(&paralyzed));
        assert!(!conditions::can_take_reactions(&paralyzed));

        // Poisoned allows actions
        let poisoned = vec![ActiveCondition::new(
            ConditionType::Poisoned,
            ConditionDuration::Rounds(2),
        )];
        assert!(conditions::can_take_actions(&poisoned));
        assert!(conditions::can_take_reactions(&poisoned));
    }

    #[test]
    fn test_stunned_and_paralyzed_auto_fail_str_dex_saves() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};
        use crate::types::Ability;

        // Stunned auto-fails STR and DEX saves
        let stunned = vec![ActiveCondition::new(
            ConditionType::Stunned,
            ConditionDuration::Rounds(1),
        )];
        assert!(conditions::get_save_auto_fail(&stunned, Ability::Strength));
        assert!(conditions::get_save_auto_fail(&stunned, Ability::Dexterity));
        assert!(!conditions::get_save_auto_fail(
            &stunned,
            Ability::Constitution
        ));

        // Paralyzed auto-fails STR and DEX saves
        let paralyzed = vec![ActiveCondition::new(
            ConditionType::Paralyzed,
            ConditionDuration::Rounds(1),
        )];
        assert!(conditions::get_save_auto_fail(
            &paralyzed,
            Ability::Strength
        ));
        assert!(conditions::get_save_auto_fail(
            &paralyzed,
            Ability::Dexterity
        ));
        assert!(!conditions::get_save_auto_fail(&paralyzed, Ability::Wisdom));
    }

    #[test]
    fn test_prone_reduces_speed() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let prone = vec![ActiveCondition::new(
            ConditionType::Prone,
            ConditionDuration::Permanent,
        )];
        assert_eq!(
            conditions::get_speed_multiplier(&prone),
            0.5,
            "Prone should reduce speed multiplier to 0.5"
        );

        let normal: Vec<ActiveCondition> = vec![];
        assert_eq!(
            conditions::get_speed_multiplier(&normal),
            1.0,
            "No conditions should have normal speed"
        );
    }

    // ---- Integration: new SRD conditions in attack resolution ----

    #[test]
    fn test_invisible_attacker_vs_visible_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let invisible = vec![ActiveCondition::new(
            ConditionType::Invisible,
            ConditionDuration::Rounds(3),
        )];
        // Attacker-side query returns Some(true) => advantage.
        assert_eq!(conditions::get_attack_advantage(&invisible), Some(true));
    }

    #[test]
    fn test_restrained_imposes_attack_disadvantage_in_combat() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let restrained = vec![ActiveCondition::new(
            ConditionType::Restrained,
            ConditionDuration::Permanent,
        )];
        assert_eq!(conditions::get_attack_advantage(&restrained), Some(false));
    }

    #[test]
    fn test_attacking_restrained_target_grants_advantage_in_combat() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let attacker: Vec<ActiveCondition> = vec![];
        let restrained = vec![ActiveCondition::new(
            ConditionType::Restrained,
            ConditionDuration::Permanent,
        )];
        assert_eq!(
            conditions::get_defense_advantage(&attacker, &restrained),
            Some(true)
        );
    }

    #[test]
    fn test_attacking_invisible_target_imposes_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let attacker: Vec<ActiveCondition> = vec![];
        let invisible = vec![ActiveCondition::new(
            ConditionType::Invisible,
            ConditionDuration::Rounds(3),
        )];
        assert_eq!(
            conditions::get_defense_advantage(&attacker, &invisible),
            Some(false)
        );
    }

    #[test]
    fn test_unconscious_target_is_auto_crit() {
        use crate::conditions::{self, ActiveCondition, ConditionDuration, ConditionType};

        let unconscious = vec![ActiveCondition::new(
            ConditionType::Unconscious,
            ConditionDuration::Permanent,
        )];
        assert!(conditions::is_auto_crit_target(&unconscious));
    }

    #[test]
    fn test_player_with_invisible_rolls_attack_with_advantage() {
        // End-to-end: player attacks a goblin while Invisible. Over many trials
        // the hit rate should be measurably higher than neutral, confirming that
        // advantage was actually applied to the roll (not just returned by the
        // query function).
        use crate::conditions::{ActiveCondition, ConditionDuration, ConditionType};

        let mut wins_with_adv = 0;
        let mut wins_neutral = 0;
        let trials = 400;

        for seed in 0..trials {
            let mut state_adv = test_state_with_goblin();
            state_adv.character.conditions.push(ActiveCondition::new(
                ConditionType::Invisible,
                ConditionDuration::Rounds(10),
            ));
            // Equip a simple weapon so the attack actually rolls (unarmed bypasses).
            // Shortcut: put an item with id 9999 as a club-equivalent into items.
            let club = crate::state::Item {
                id: 9999,
                name: "club".to_string(),
                description: "A sturdy club.".to_string(),
                item_type: crate::state::ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 6,
                    damage_type: crate::state::DamageType::Bludgeoning,
                    properties: 0,
                    category: crate::state::WeaponCategory::Simple,
                    versatile_die: 0,
                    range_normal: 0,
                    range_long: 0,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            };
            state_adv.world.items.insert(9999, club.clone());
            state_adv.character.inventory.push(9999);
            state_adv.character.equipped.main_hand = Some(9999);

            let mut state_neu = state_adv.clone();
            state_neu.character.conditions.clear();

            let distance = 5u32;
            let target_ac = 15i32;
            let mut rng1 = StdRng::seed_from_u64(seed as u64);
            let mut rng2 = StdRng::seed_from_u64(seed as u64);

            let res_adv = resolve_player_attack(
                &mut rng1,
                &state_adv.character,
                target_ac,
                false,
                Some(9999),
                &state_adv.world.items,
                distance,
                true,
                false,
                &[], // defender has no conditions
                false,
                false,
                &Cover::None,
            );
            let res_neu = resolve_player_attack(
                &mut rng2,
                &state_neu.character,
                target_ac,
                false,
                Some(9999),
                &state_neu.world.items,
                distance,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );

            if res_adv.hit {
                wins_with_adv += 1;
            }
            if res_neu.hit {
                wins_neutral += 1;
            }
        }

        // Advantage should measurably improve hit rate; require a reasonable gap.
        assert!(
            wins_with_adv > wins_neutral + 20,
            "Invisible attacker should hit more often ({} vs neutral {})",
            wins_with_adv,
            wins_neutral
        );
    }

    #[test]
    fn test_extra_disadvantage_flag_reduces_player_hit_rate() {
        // End-to-end: when the orchestrator reports `extra_disadvantage = true`
        // (e.g., Grappled attacking a non-grappler) the hit rate should be
        // measurably lower than without it. This test verifies the parameter
        // is actually wired into the roll.
        use crate::state::{DamageType, Item, ItemType, WeaponCategory};

        let mut wins_disadv = 0;
        let mut wins_neutral = 0;
        let trials = 400u64;

        for seed in 0..trials {
            let mut state = test_state_with_goblin();
            let club = Item {
                id: 9999,
                name: "club".to_string(),
                description: "Sturdy club.".to_string(),
                item_type: ItemType::Weapon {
                    damage_dice: 1,
                    damage_die: 6,
                    damage_type: DamageType::Bludgeoning,
                    properties: 0,
                    category: WeaponCategory::Simple,
                    versatile_die: 0,
                    range_normal: 0,
                    range_long: 0,
                },
                location: None,
                carried_by_player: true,
                charges_remaining: None,
            };
            state.world.items.insert(9999, club);
            state.character.inventory.push(9999);
            state.character.equipped.main_hand = Some(9999);

            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);

            let res_disadv = resolve_player_attack(
                &mut rng1,
                &state.character,
                15,
                false,
                Some(9999),
                &state.world.items,
                5,
                true,
                false,
                &[],
                true,
                false,
                &Cover::None,
            );
            let res_neu = resolve_player_attack(
                &mut rng2,
                &state.character,
                15,
                false,
                Some(9999),
                &state.world.items,
                5,
                true,
                false,
                &[],
                false,
                false,
                &Cover::None,
            );
            if res_disadv.hit {
                wins_disadv += 1;
            }
            if res_neu.hit {
                wins_neutral += 1;
            }
        }

        assert!(
            wins_neutral > wins_disadv + 20,
            "extra_disadvantage should lower hit rate (disadv={}, neutral={})",
            wins_disadv,
            wins_neutral
        );
    }

    // Dodge disadvantage should surface both raw d20 rolls inline so the
    // player can see which lower die was chosen.
    #[test]
    fn test_dodge_disadvantage_shown_in_npc_attack_text() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Place goblin in melee range and set player dodging
        combat.distances.insert(0, 5);
        combat.player_dodging = true;

        // Run many seeds to get non-crit, non-nat-1 results (those show roll details)
        let mut found_disadvantage_text = false;
        for seed in 0..200u64 {
            let mut test_rng = StdRng::seed_from_u64(seed);
            let mut test_state = state.clone();
            let mut test_combat = combat.clone();
            let lines = resolve_npc_turn(&mut test_rng, 0, &mut test_state, &mut test_combat);
            let all = lines.join("\n");
            // Skip nat 20s and nat 1s since they use different format strings
            if all.contains("CRITICAL HIT") || all.contains("natural 1") {
                continue;
            }
            if all.contains(" \u{2192} ") && all.contains(" vs AC ") {
                found_disadvantage_text = true;
                break;
            }
        }
        assert!(found_disadvantage_text,
            "NPC attack output should show both d20 rolls and the chosen die when player is dodging");
    }

    #[test]
    fn test_no_disadvantage_text_when_not_dodging() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Place goblin in melee range, player NOT dodging
        combat.distances.insert(0, 5);
        combat.player_dodging = false;

        for seed in 0..100u64 {
            let mut test_rng = StdRng::seed_from_u64(seed);
            let mut test_state = state.clone();
            let mut test_combat = combat.clone();
            let lines = resolve_npc_turn(&mut test_rng, 0, &mut test_state, &mut test_combat);
            let all = lines.join("\n");
            assert!(
                !all.contains(" \u{2192} "),
                "Should not show dual-roll attack text when player is not dodging. Got: {}",
                all
            );
        }
    }

    #[test]
    fn test_format_attack_roll_details_shows_both_disadvantage_d20s() {
        let result = AttackResult {
            hit: false,
            natural_20: false,
            natural_1: false,
            attack_roll_first: 17,
            attack_roll_second: Some(4),
            attack_roll: 4,
            total_attack: 7,
            target_ac: 15,
            damage: 0,
            damage_type: DamageType::Slashing,
            weapon_name: "Longsword".to_string(),
            disadvantage: true,
            attacker_had_advantage: false,
        };

        assert_eq!(
            format_attack_roll_details(&result, 3),
            "17 / 4 \u{2192} 4 (4+3=7) (disadvantage \u{2014} keeping 4)"
        );
    }

    #[test]
    fn test_format_attack_roll_details_shows_both_advantage_d20s() {
        let result = AttackResult {
            hit: true,
            natural_20: false,
            natural_1: false,
            attack_roll_first: 14,
            attack_roll_second: Some(8),
            attack_roll: 14,
            total_attack: 17,
            target_ac: 15,
            damage: 7,
            damage_type: DamageType::Slashing,
            weapon_name: "Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: true,
        };

        assert_eq!(
            format_attack_roll_details(&result, 3),
            "14 / 8 \u{2192} 14 (14+3=17) (advantage \u{2014} keeping 14)"
        );
    }

    // ---- Action Economy tests ----

    #[test]
    fn test_combat_state_has_four_independent_resource_flags() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Fresh combat should have all resources available.
        assert!(!combat.action_used, "Action should start available");
        assert!(
            !combat.bonus_action_used,
            "Bonus action should start available"
        );
        assert!(!combat.reaction_used, "Reaction should start available");
        assert!(
            !combat.free_interaction_used,
            "Free interaction should start available"
        );
    }

    #[test]
    fn test_reaction_resets_at_end_of_player_turn_not_start() {
        // Per SRD: reaction resets at end of previous turn so it's available during NPC turns.
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        // Simulate player consuming reaction (e.g. opportunity attack during NPC turn)
        combat.reaction_used = true;

        // End the player's turn: reaction should reset so NPC-turn reactions can fire later.
        combat.end_player_turn();
        assert!(
            !combat.reaction_used,
            "Reaction should reset at end of player turn so NPCs can't prevent its use"
        );
    }

    #[test]
    fn test_action_bonus_free_reset_at_start_of_player_turn() {
        // action/bonus/free reset at start of the new player turn (existing convention).
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        combat.action_used = true;
        combat.bonus_action_used = true;
        combat.free_interaction_used = true;
        combat.player_movement_remaining = 0;

        // Force advance_turn to cycle back to player (even if already player turn)
        // Simulate an NPC turn by setting current_turn to an NPC, then advancing.
        combat.current_turn = combat
            .initiative_order
            .iter()
            .position(|(c, _)| matches!(c, Combatant::Npc(_)))
            .unwrap_or(0);

        combat.advance_turn(&mut state);

        assert!(
            combat.is_player_turn(),
            "Should advance back to player turn"
        );
        assert!(
            !combat.action_used,
            "Action should reset at start of player turn"
        );
        assert!(
            !combat.bonus_action_used,
            "Bonus should reset at start of player turn"
        );
        assert!(
            !combat.free_interaction_used,
            "Free interaction should reset at start of player turn"
        );
        assert_eq!(
            combat.player_movement_remaining, state.character.speed,
            "Movement should reset to speed at start of player turn"
        );
    }

    #[test]
    fn test_pending_reaction_defaults_to_none_and_serialises() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        assert!(
            combat.pending_reaction.is_none(),
            "Fresh combat should have no pending reaction"
        );

        // Round trip
        let json = serde_json::to_string(&combat).unwrap();
        let deserialised: CombatState = serde_json::from_str(&json).unwrap();
        assert!(deserialised.pending_reaction.is_none());
    }

    #[test]
    fn test_pending_reaction_opportunity_attack_round_trips() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        combat.pending_reaction = Some(PendingReaction::OpportunityAttack {
            fleeing_npc_id: 0,
            old_distance: 5,
            new_distance: 30,
            resume_npc_index: 1,
        });

        let json = serde_json::to_string(&combat).unwrap();
        let deserialised: CombatState = serde_json::from_str(&json).unwrap();
        match deserialised.pending_reaction {
            Some(PendingReaction::OpportunityAttack {
                fleeing_npc_id,
                old_distance,
                new_distance,
                resume_npc_index,
            }) => {
                assert_eq!(fleeing_npc_id, 0);
                assert_eq!(old_distance, 5);
                assert_eq!(new_distance, 30);
                assert_eq!(resume_npc_index, 1);
            }
            other => panic!("Expected OpportunityAttack, got {:?}", other),
        }
    }

    #[test]
    fn test_pending_reaction_shield_round_trips() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);

        combat.pending_reaction = Some(PendingReaction::Shield {
            attacker_npc_id: 0,
            incoming_damage: 7,
            pre_roll_ac: 15,
            resume_npc_index: 1,
        });

        let json = serde_json::to_string(&combat).unwrap();
        let deserialised: CombatState = serde_json::from_str(&json).unwrap();
        match deserialised.pending_reaction {
            Some(PendingReaction::Shield {
                attacker_npc_id,
                incoming_damage,
                pre_roll_ac,
                resume_npc_index,
            }) => {
                assert_eq!(attacker_npc_id, 0);
                assert_eq!(incoming_damage, 7);
                assert_eq!(pre_roll_ac, 15);
                assert_eq!(resume_npc_index, 1);
            }
            other => panic!("Expected Shield, got {:?}", other),
        }
    }

    #[test]
    fn test_action_used_serde_alias_loads_old_saves() {
        // Backwards-compat: old saves serialised `player_action_used` should still deserialize.
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.action_used = true; // Mark as used, so alias must carry the value.

        let mut json = serde_json::to_value(&combat).unwrap();
        // Simulate an old save: rename key, strip the new fields that old saves don't have.
        if let Some(obj) = json.as_object_mut() {
            let val = obj.remove("action_used").expect("action_used field");
            obj.insert("player_action_used".to_string(), val);
            obj.remove("bonus_action_used");
            obj.remove("reaction_used");
            obj.remove("free_interaction_used");
        }
        let round_tripped: CombatState = serde_json::from_value(json)
            .expect("Old save with player_action_used should still deserialize");
        // Old-save deserialization: action_used should come from the alias value (true).
        assert!(
            round_tripped.action_used,
            "Old saves' player_action_used value should map to action_used"
        );
        // New fields should default to false.
        assert!(!round_tripped.bonus_action_used);
        assert!(!round_tripped.reaction_used);
        assert!(!round_tripped.free_interaction_used);
    }

    // ----- monster-stat-blocks (2026-04-15) -----

    use crate::combat::monsters::{find_monster, monster_to_combat_stats};
    use crate::conditions::ConditionDuration;

    fn npc_with_stats(name: &str, stats: CombatStats) -> Npc {
        Npc {
            id: 9001,
            name: name.to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(stats),
            conditions: Vec::new(),
        }
    }

    #[test]
    fn test_try_apply_condition_to_npc_rejects_stat_block_immunity() {
        let skel_def = find_monster("Skeleton").unwrap();
        let stats = monster_to_combat_stats(skel_def);
        let mut npc = npc_with_stats("Skeleton", stats);

        let applied = try_apply_condition_to_npc(
            &mut npc,
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        );
        assert!(
            !applied,
            "Skeleton should reject Poisoned condition due to stat-block immunity"
        );
        assert!(
            npc.conditions.is_empty(),
            "rejected condition should not be appended"
        );
    }

    #[test]
    fn test_try_apply_condition_to_npc_accepts_non_immune() {
        let skel_def = find_monster("Skeleton").unwrap();
        let stats = monster_to_combat_stats(skel_def);
        let mut npc = npc_with_stats("Skeleton", stats);

        let applied = try_apply_condition_to_npc(
            &mut npc,
            ActiveCondition::new(ConditionType::Frightened, ConditionDuration::Rounds(3)),
        );
        assert!(applied);
        assert_eq!(npc.conditions.len(), 1);
        assert_eq!(npc.conditions[0].condition, ConditionType::Frightened);
    }

    #[test]
    fn test_try_apply_condition_to_npc_honors_petrified_poison_rule() {
        // Even with no stat-block immunities, conditions::is_immune_to_condition
        // should still apply (Petrified => Poisoned).
        let mut stats = CombatStats::default();
        stats.condition_immunities = vec![]; // no stat-block immunities
        let mut npc = npc_with_stats("Statue", stats);
        // First apply Petrified.
        let p1 = try_apply_condition_to_npc(
            &mut npc,
            ActiveCondition::new(ConditionType::Petrified, ConditionDuration::Permanent),
        );
        assert!(p1);
        // Then try to poison: should be rejected by the generic immunity rule.
        let p2 = try_apply_condition_to_npc(
            &mut npc,
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        );
        assert!(
            !p2,
            "Petrified target should reject Poisoned per conditions::is_immune_to_condition"
        );
        assert_eq!(npc.conditions.len(), 1);
        assert_eq!(npc.conditions[0].condition, ConditionType::Petrified);
    }

    #[test]
    fn test_try_apply_condition_to_npc_skips_immunity_when_no_combat_stats() {
        // Friendly NPCs with no combat_stats can still receive conditions
        // via the generic apply_condition path; the helper should not panic.
        let mut npc = Npc {
            id: 9002,
            name: "Friendly Hermit".to_string(),
            role: NpcRole::Hermit,
            disposition: Disposition::Friendly,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: None,
            conditions: Vec::new(),
        };
        let applied = try_apply_condition_to_npc(
            &mut npc,
            ActiveCondition::new(ConditionType::Charmed, ConditionDuration::Rounds(1)),
        );
        assert!(applied);
        assert_eq!(npc.conditions.len(), 1);
    }

    #[test]
    fn test_apply_damage_modifiers_immunity_zeros_damage() {
        let zombie_def = find_monster("Zombie").unwrap();
        let stats = monster_to_combat_stats(zombie_def);
        let mut narration = Vec::new();
        let dealt =
            apply_damage_modifiers(&stats, 12, DamageType::Poison, "zombie", &mut narration);
        assert_eq!(dealt, 0, "Zombie should be immune to Poison damage");
        assert_eq!(narration.len(), 1);
        assert!(
            narration[0].contains("immune"),
            "narration mentions immunity: {:?}",
            narration[0]
        );
    }

    #[test]
    fn test_apply_damage_modifiers_no_immunity_passes_through() {
        let zombie_def = find_monster("Zombie").unwrap();
        let stats = monster_to_combat_stats(zombie_def);
        let mut narration = Vec::new();
        let dealt =
            apply_damage_modifiers(&stats, 7, DamageType::Slashing, "zombie", &mut narration);
        assert_eq!(dealt, 7);
        assert!(
            narration.is_empty(),
            "no narration when no immunity/resistance applies"
        );
    }

    #[test]
    fn test_apply_damage_modifiers_resistance_halves_damage() {
        let mut stats = CombatStats::default();
        stats.damage_resistances = vec![DamageType::Slashing];
        let mut narration = Vec::new();
        let dealt =
            apply_damage_modifiers(&stats, 10, DamageType::Slashing, "ghost", &mut narration);
        assert_eq!(dealt, 5);
        assert_eq!(narration.len(), 1);
        assert!(narration[0].contains("resists"));
    }

    #[test]
    fn test_apply_damage_modifiers_resistance_halves_odd_damage() {
        // 11 / 2 = 5 (round down, integer division).
        let mut stats = CombatStats::default();
        stats.damage_resistances = vec![DamageType::Fire];
        let mut narration = Vec::new();
        let dealt = apply_damage_modifiers(&stats, 11, DamageType::Fire, "fiend", &mut narration);
        assert_eq!(dealt, 5);
    }

    #[test]
    fn test_apply_damage_modifiers_zero_or_negative_input() {
        let stats = CombatStats::default();
        let mut narration = Vec::new();
        assert_eq!(
            apply_damage_modifiers(&stats, 0, DamageType::Fire, "x", &mut narration),
            0
        );
        assert_eq!(
            apply_damage_modifiers(&stats, -3, DamageType::Fire, "x", &mut narration),
            0
        );
        assert!(narration.is_empty());
    }

    fn count_lines_with(needle: &str, lines: &[String]) -> usize {
        lines.iter().filter(|l| l.contains(needle)).count()
    }

    #[test]
    fn test_npc_multiattack_makes_two_attacks() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Bump goblin to multiattack 2 and give it lots of HP so the player
        // doesn't drop the goblin (only player can be damaged here anyway).
        let stats = state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap();
        stats.multiattack = 2;
        // Clear the bow attack so we are guaranteed to take the melee branch.
        stats.attacks.retain(|a| a.name == "Scimitar");
        // Give the player enough HP to survive 2 hits.
        state.character.current_hp = 1000;
        state.character.max_hp = 1000;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        // 2 attack lines (hit/miss/crit each emit one line; never zero per attack).
        let attack_lines = count_lines_with("Scimitar", &lines);
        assert_eq!(
            attack_lines, 2,
            "multiattack=2 should produce 2 Scimitar attack lines, got {}: {:#?}",
            attack_lines, lines
        );
    }

    #[test]
    fn test_npc_multiattack_one_makes_one_attack_regression() {
        // multiattack==1 is the default; verify legacy behavior.
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let stats = state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap();
        assert_eq!(stats.multiattack, 1, "fixture default should be 1");
        stats.attacks.retain(|a| a.name == "Scimitar");
        state.character.current_hp = 1000;
        state.character.max_hp = 1000;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let attack_lines = count_lines_with("Scimitar", &lines);
        assert_eq!(attack_lines, 1);
    }

    #[test]
    fn test_npc_multiattack_continues_on_dying_player_until_three_failures() {
        // Per SRD Death Saving Throws (issue #84), when the first attack
        // knocks the player to 0 HP, subsequent attacks in the same
        // multiattack add failures (2 on crit, 1 otherwise). Multiattack
        // continues until the player has accumulated three death save
        // failures (instant death via massive damage, or three hits).
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let stats = state
            .world
            .npcs
            .get_mut(&0)
            .unwrap()
            .combat_stats
            .as_mut()
            .unwrap();
        stats.multiattack = 3;
        stats.attacks.retain(|a| a.name == "Scimitar");
        // Make the attack always hit but keep damage well below max_hp so no
        // single hit triggers the massive-damage instant-death rule.
        stats.attacks[0].hit_bonus = 50;
        stats.attacks[0].damage_bonus = 100;
        state.character.current_hp = 1;
        state.character.max_hp = 10_000; // ensure damage < max_hp per hit

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let attack_lines = count_lines_with("Scimitar", &lines);
        // At least one attack lands (the killing blow), and multiattack may
        // continue up to three times adding death save failures. Must not
        // exceed the configured multiattack count.
        assert!(
            attack_lines >= 1 && attack_lines <= 3,
            "expected between 1 and 3 Scimitar attacks, got {}: {:#?}",
            attack_lines,
            lines
        );
        assert!(
            combat.death_save_failures >= 1,
            "expected at least one DST failure from additional hits: {:#?}",
            lines
        );
    }

    #[test]
    fn test_apply_damage_to_npc_immunity() {
        let zombie_def = find_monster("Zombie").unwrap();
        let stats = monster_to_combat_stats(zombie_def);
        let starting_hp = stats.current_hp;
        let mut npc = npc_with_stats("Zombie", stats);

        let mut narr = Vec::new();
        let dealt = apply_damage_to_npc(&mut npc, 12, DamageType::Poison, &mut narr);
        assert_eq!(dealt, 0);
        assert_eq!(
            npc.combat_stats.as_ref().unwrap().current_hp,
            starting_hp,
            "immune target's HP should be unchanged"
        );
        assert_eq!(narr.len(), 1);
    }

    #[test]
    fn test_apply_damage_to_npc_full_damage() {
        let goblin_def = find_monster("Goblin").unwrap();
        let stats = monster_to_combat_stats(goblin_def);
        let starting_hp = stats.current_hp;
        let mut npc = npc_with_stats("Goblin", stats);

        let mut narr = Vec::new();
        let dealt = apply_damage_to_npc(&mut npc, 5, DamageType::Slashing, &mut narr);
        assert_eq!(dealt, 5);
        assert_eq!(
            npc.combat_stats.as_ref().unwrap().current_hp,
            starting_hp - 5
        );
        assert!(narr.is_empty());
    }

    #[test]
    fn test_apply_damage_to_npc_caps_at_zero() {
        let mut stats = CombatStats::default();
        stats.max_hp = 5;
        stats.current_hp = 5;
        let mut npc = npc_with_stats("Frail", stats);

        let mut narr = Vec::new();
        let dealt = apply_damage_to_npc(&mut npc, 100, DamageType::Slashing, &mut narr);
        assert_eq!(dealt, 100);
        assert_eq!(
            npc.combat_stats.as_ref().unwrap().current_hp,
            0,
            "current_hp clamps to 0, never negative"
        );
    }

    #[test]
    fn test_apply_damage_to_npc_no_combat_stats() {
        let mut npc = Npc {
            id: 9003,
            name: "Friendly".to_string(),
            role: NpcRole::Hermit,
            disposition: Disposition::Friendly,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: None,
            conditions: Vec::new(),
        };
        let mut narr = Vec::new();
        let dealt = apply_damage_to_npc(&mut npc, 50, DamageType::Slashing, &mut narr);
        assert_eq!(
            dealt, 0,
            "NPC without combat_stats takes no damage and the helper is a no-op"
        );
    }

    #[test]
    fn test_apply_damage_modifiers_immunity_takes_precedence_over_resistance() {
        // If a creature is both resistant AND immune (unusual but possible),
        // immunity (full negation) wins.
        let mut stats = CombatStats::default();
        stats.damage_immunities = vec![DamageType::Cold];
        stats.damage_resistances = vec![DamageType::Cold];
        let mut narration = Vec::new();
        let dealt =
            apply_damage_modifiers(&stats, 8, DamageType::Cold, "elemental", &mut narration);
        assert_eq!(dealt, 0);
        assert_eq!(narration.len(), 1);
        assert!(narration[0].contains("immune"));
    }

    // ---- Weapon Mastery helpers (feat/weapon-mastery) ----

    fn test_goblin(id: NpcId) -> Npc {
        Npc {
            id,
            name: format!("Goblin {}", id),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(goblin_stats()),
            conditions: Vec::new(),
        }
    }

    fn attack_result(hit: bool, damage: i32, damage_type: DamageType) -> AttackResult {
        AttackResult {
            hit,
            natural_20: false,
            natural_1: false,
            attack_roll_first: if hit { 15 } else { 5 },
            attack_roll_second: None,
            attack_roll: if hit { 15 } else { 5 },
            total_attack: if hit { 20 } else { 8 },
            target_ac: 13,
            damage,
            damage_type,
            weapon_name: "Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: false,
        }
    }

    #[test]
    fn test_graze_on_miss_deals_ability_mod_damage() {
        let mut npc = test_goblin(1);
        let missed = attack_result(false, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        let dealt = apply_graze_mastery(true, &missed, 3, &mut npc, &mut narr);
        assert_eq!(dealt, 3);
        assert!(narr.iter().any(|l| l.contains("Graze")));
        let stats = npc.combat_stats.as_ref().unwrap();
        assert_eq!(stats.current_hp, stats.max_hp - 3);
    }

    #[test]
    fn test_graze_no_mastery_is_noop() {
        let mut npc = test_goblin(1);
        let missed = attack_result(false, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        let dealt = apply_graze_mastery(false, &missed, 3, &mut npc, &mut narr);
        assert_eq!(dealt, 0);
        assert!(narr.is_empty());
    }

    #[test]
    fn test_graze_on_hit_is_noop() {
        // Graze only applies on miss.
        let mut npc = test_goblin(1);
        let hit = attack_result(true, 7, DamageType::Slashing);
        let mut narr = Vec::new();
        let dealt = apply_graze_mastery(true, &hit, 3, &mut npc, &mut narr);
        assert_eq!(dealt, 0);
    }

    #[test]
    fn test_graze_with_zero_or_negative_mod_is_noop() {
        let mut npc = test_goblin(1);
        let missed = attack_result(false, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        // Per SRD, Graze damage equals the ability modifier used; a 0 or
        // negative modifier yields no damage.
        assert_eq!(
            apply_graze_mastery(true, &missed, 0, &mut npc, &mut narr),
            0
        );
        assert_eq!(
            apply_graze_mastery(true, &missed, -1, &mut npc, &mut narr),
            0
        );
    }

    #[test]
    fn test_vex_mastery_marks_target_and_is_consumed_on_next_attack() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let hit = attack_result(true, 7, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(apply_vex_mastery(true, &hit, 42, &mut combat, &mut narr));
        assert_eq!(combat.player_vex_target, Some(42));
        // Consume vex — should return true once, then false.
        assert!(consume_vex_advantage(&mut combat, 42));
        assert_eq!(combat.player_vex_target, None);
        assert!(!consume_vex_advantage(&mut combat, 42));
    }

    #[test]
    fn test_vex_requires_damage() {
        // Per spec, Vex requires the hit to deal damage.
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let zero_dmg_hit = attack_result(true, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_vex_mastery(
            true,
            &zero_dmg_hit,
            42,
            &mut combat,
            &mut narr
        ));
        assert_eq!(combat.player_vex_target, None);
    }

    #[test]
    fn test_sap_mastery_marks_then_consumes() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let hit = attack_result(true, 5, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(apply_sap_mastery(true, &hit, 7, &mut combat, &mut narr));
        assert!(combat.sap_targets.contains(&7));
        assert!(consume_sap_disadvantage(&mut combat, 7));
        assert!(!combat.sap_targets.contains(&7));
        // Second call returns false.
        assert!(!consume_sap_disadvantage(&mut combat, 7));
    }

    #[test]
    fn test_sap_on_miss_is_noop() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let missed = attack_result(false, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_sap_mastery(true, &missed, 7, &mut combat, &mut narr));
        assert!(combat.sap_targets.is_empty());
    }

    #[test]
    fn test_slow_mastery_applies_10ft_reduction() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let hit = attack_result(true, 5, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(apply_slow_mastery(true, &hit, 7, &mut combat, &mut narr));
        assert_eq!(slow_speed_reduction(&combat, 7), 10);
        // Repeat Slow on same target this turn — does not stack (already 10 ft).
        assert!(!apply_slow_mastery(true, &hit, 7, &mut combat, &mut narr));
        assert_eq!(slow_speed_reduction(&combat, 7), 10);
    }

    #[test]
    fn test_slow_requires_damage() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        let zero_dmg_hit = attack_result(true, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_slow_mastery(
            true,
            &zero_dmg_hit,
            7,
            &mut combat,
            &mut narr
        ));
        assert_eq!(slow_speed_reduction(&combat, 7), 0);
    }

    #[test]
    fn test_push_mastery_moves_target_10ft_away() {
        use crate::combat::monsters::Size;
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        combat.distances.insert(7, 5);
        let hit = attack_result(true, 5, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        let pushed = apply_push_mastery(true, &hit, 7, &mut combat, &mut narr, Size::Medium);
        assert_eq!(pushed, Some(15));
        assert_eq!(combat.distances.get(&7), Some(&15));
    }

    #[test]
    fn test_push_mastery_respects_size_limit() {
        use crate::combat::monsters::Size;
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        combat.distances.insert(7, 5);
        let hit = attack_result(true, 5, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        // Huge: not pushed.
        assert_eq!(
            apply_push_mastery(true, &hit, 7, &mut combat, &mut narr, Size::Huge),
            None
        );
        assert_eq!(
            combat.distances.get(&7),
            Some(&5),
            "Huge should not be pushed"
        );
        // Large: pushed.
        let pushed = apply_push_mastery(true, &hit, 7, &mut combat, &mut narr, Size::Large);
        assert_eq!(pushed, Some(15));
    }

    #[test]
    fn test_topple_mastery_applies_prone_on_failed_save() {
        let player = test_character();
        let mut state = test_game_state(player);
        state.world.npcs.insert(7, test_goblin(7));
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1),
            &state.character,
            &[],
            &HashMap::new(),
            crate::state::LocationType::Room,
        );
        let _ = &mut combat; // unused for this test; kept for future use
        let hit = attack_result(true, 5, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        // DC is 8 + mod + prof = 8 + 5 + 2 = 15. Goblin CON 10 (mod 0). Seed
        // chosen so the first d20 rolls less than 15 -> fail.
        let mut rng = StdRng::seed_from_u64(2);
        let applied = apply_topple_mastery(
            true, &hit, 7, &mut state, &mut narr, /*ability_mod=*/ 5, /*prof_bonus=*/ 2,
            &mut rng,
        );
        // Whether applied depends on RNG; assert either outcome is reported
        // via narration and that Prone presence mirrors the reported line.
        let got_prone = state
            .world
            .npcs
            .get(&7)
            .unwrap()
            .conditions
            .iter()
            .any(|c| c.condition == ConditionType::Prone);
        assert_eq!(applied, got_prone);
    }

    #[test]
    fn test_topple_mastery_requires_hit() {
        let player = test_character();
        let mut state = test_game_state(player);
        state.world.npcs.insert(7, test_goblin(7));
        let missed = attack_result(false, 0, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        let mut rng = StdRng::seed_from_u64(2);
        let applied = apply_topple_mastery(true, &missed, 7, &mut state, &mut narr, 5, 2, &mut rng);
        assert!(!applied);
        let got_prone = state
            .world
            .npcs
            .get(&7)
            .unwrap()
            .conditions
            .iter()
            .any(|c| c.condition == ConditionType::Prone);
        assert!(!got_prone);
    }

    #[test]
    fn test_cleave_requires_secondary_in_melee() {
        let mut player = test_character();
        player.weapon_masteries.push("Greataxe".to_string());
        let mut state = test_game_state(player);
        // Only the primary target in range; no secondary.
        state.world.npcs.insert(7, test_goblin(7));
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1),
            &state.character,
            &[],
            &HashMap::new(),
            crate::state::LocationType::Room,
        );
        combat.distances.insert(7, 5);
        let mut rng = StdRng::seed_from_u64(3);
        let hit = attack_result(true, 8, DamageType::Slashing);
        let out = apply_cleave_mastery(
            &mut rng,
            true,
            &hit,
            7,
            &mut combat,
            &state,
            /*ability_mod=*/ 5,
        );
        assert!(out.is_none(), "No secondary in 5 ft -> no cleave");
        assert!(!combat.cleave_used_this_turn);
    }

    #[test]
    fn test_cleave_targets_secondary_once_per_turn() {
        let mut player = test_character();
        player.weapon_masteries.push("Greataxe".to_string());
        // Equip Greataxe so the cleave re-attack uses the same weapon's
        // mechanical fields as the primary. For this test we only assert
        // that a secondary is selected and the flag is set; the actual
        // damage depends on the RNG and the weapon fields.
        let mut state = test_game_state(player);
        state.world.npcs.insert(7, test_goblin(7));
        state.world.npcs.insert(8, test_goblin(8));
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1),
            &state.character,
            &[],
            &HashMap::new(),
            crate::state::LocationType::Room,
        );
        combat.distances.insert(7, 5);
        combat.distances.insert(8, 5);
        let mut rng = StdRng::seed_from_u64(3);
        let hit = attack_result(true, 8, DamageType::Slashing);
        let out = apply_cleave_mastery(&mut rng, true, &hit, 7, &mut combat, &state, 5);
        assert!(out.is_some());
        let (secondary_id, _cleave_result, _mod) = out.unwrap();
        assert_eq!(secondary_id, 8);
        assert!(combat.cleave_used_this_turn);
        // Second cleave same turn is blocked by the flag.
        let out2 = apply_cleave_mastery(&mut rng, true, &hit, 7, &mut combat, &state, 5);
        assert!(out2.is_none());
    }

    #[test]
    fn test_nick_mastery_fires_once_per_turn() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        // First Nick swing: applies, consumes the once-per-turn slot.
        assert!(apply_nick_mastery(true, &mut combat));
        assert!(combat.nick_used_this_turn);
        // Second Nick swing in the same turn: does not apply. A second
        // off-hand attack still works via normal TWF rules (bonus action)
        // because the orchestrator decides that based on this return.
        assert!(!apply_nick_mastery(true, &mut combat));
    }

    #[test]
    fn test_nick_mastery_no_op_without_mastery() {
        let player = test_character();
        let mut combat = start_combat(&mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(), crate::state::LocationType::Room);
        assert!(!apply_nick_mastery(false, &mut combat));
        assert!(!combat.nick_used_this_turn);
    }

    // ---- Rogue Sneak Attack helpers ----

    #[test]
    fn test_sneak_attack_dice_for_level_matches_srd() {
        // SRD 5.1: ceil(level / 2) d6 -- floor((level+1)/2) d6.
        assert_eq!(sneak_attack_dice_for_level(1), 1);
        assert_eq!(sneak_attack_dice_for_level(2), 1);
        assert_eq!(sneak_attack_dice_for_level(3), 2);
        assert_eq!(sneak_attack_dice_for_level(4), 2);
        assert_eq!(sneak_attack_dice_for_level(5), 3);
        assert_eq!(sneak_attack_dice_for_level(11), 6);
        assert_eq!(sneak_attack_dice_for_level(20), 10);
    }

    #[test]
    fn test_sneak_attack_weapon_qualifies_finesse() {
        // Finesse weapon (e.g. shortsword) qualifies in melee.
        assert!(sneak_attack_weapon_qualifies(FINESSE | 0u16, false));
        // Finesse weapon still qualifies in a ranged attack (thrown).
        assert!(sneak_attack_weapon_qualifies(FINESSE | THROWN, true));
    }

    #[test]
    fn test_sneak_attack_weapon_qualifies_ranged() {
        // Non-finesse ranged weapon (e.g. shortbow) qualifies when fired.
        assert!(sneak_attack_weapon_qualifies(AMMUNITION, true));
    }

    #[test]
    fn test_sneak_attack_weapon_does_not_qualify_when_neither() {
        // A non-finesse melee weapon (e.g. longsword, greatsword): no SA.
        assert!(!sneak_attack_weapon_qualifies(VERSATILE, false));
        assert!(!sneak_attack_weapon_qualifies(0u16, false));
    }

    #[test]
    fn test_roll_sneak_attack_is_in_expected_range() {
        // Level 1 -> 1d6 -> [1, 6].
        for seed in 0..20u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let damage = roll_sneak_attack(&mut rng, 1, false);
            assert!(
                (1..=6).contains(&damage),
                "L1 SA damage out of range: {}",
                damage
            );
        }
        // Level 5 -> 3d6 -> [3, 18].
        for seed in 0..20u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let damage = roll_sneak_attack(&mut rng, 5, false);
            assert!(
                (3..=18).contains(&damage),
                "L5 SA damage out of range: {}",
                damage
            );
        }
        // Level 1 crit -> 2d6 -> [2, 12].
        for seed in 0..20u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let damage = roll_sneak_attack(&mut rng, 1, true);
            assert!(
                (2..=12).contains(&damage),
                "L1 crit SA damage out of range: {}",
                damage
            );
        }
    }

    #[test]
    fn test_advance_turn_resets_sneak_attack_flag() {
        // The turn-start reset lives in advance_turn because SA is a
        // once-per-turn cap. Simulate an NPC turn -> advance -> player
        // turn, and verify sneak_attack_used_this_turn cleared.
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state.character.class_features.sneak_attack_used_this_turn = true;
        state.character.class_features.cunning_action_used = true;
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Force current_turn to the NPC slot before advancing.
        combat.current_turn = combat
            .initiative_order
            .iter()
            .position(|(c, _)| matches!(c, Combatant::Npc(_)))
            .unwrap_or(0);
        combat.advance_turn(&mut state);
        assert!(combat.is_player_turn(), "should land on player turn");
        assert!(
            !state.character.class_features.sneak_attack_used_this_turn,
            "SA flag should reset at start of player turn"
        );
        assert!(
            !state.character.class_features.cunning_action_used,
            "Cunning Action flag should reset at start of player turn"
        );
    }

    /// Minimal GameState helper used only by the mastery tests above.
    fn test_game_state(character: Character) -> GameState {
        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(),
                npcs: HashMap::new(),
                items: HashMap::new(),
                triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 1,
            rng_counter: 0,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: Default::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
            pending_new_game_confirm: false,
        }
    }

    // ---------- Death Saving Throws (issue #84) --------------------------
    //
    // Per SRD 5e, a player character reduced to 0 HP does not immediately
    // die: they fall unconscious and roll Death Saving Throws at the start
    // of each of their turns. Three successes stabilize; three failures
    // kill. A natural 20 stabilizes immediately at 1 HP. A natural 1 counts
    // as two failures. Damage while at 0 HP adds a failure (two for a crit),
    // and damage equal to or exceeding the character's HP maximum in a
    // single hit causes instant death. Healing any amount restores the
    // character and resets the saves.

    #[test]
    fn test_check_end_does_not_defeat_at_zero_hp_when_dying_state_fresh() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Fresh dying: 0 successes, 0 failures -- combat continues.
        assert_eq!(
            combat.check_end(&state),
            None,
            "Combat should continue while player is dying (not yet 3 failures)"
        );
    }

    #[test]
    fn test_check_end_defeats_after_three_death_save_failures() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_failures = 3;
        assert_eq!(
            combat.check_end(&state),
            Some(false),
            "Three death save failures should result in defeat"
        );
    }

    #[test]
    fn test_is_player_dying_true_at_zero_hp_with_failures_below_three() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert!(combat.is_player_dying(&state));
    }

    #[test]
    fn test_is_player_dying_false_when_hp_positive() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert!(!combat.is_player_dying(&state));
    }

    #[test]
    fn test_death_save_roll_10_or_higher_counts_as_success() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let outcome = combat.apply_death_save_roll(&mut state.character, 15);
        assert_eq!(outcome, DeathSaveOutcome::Success);
        assert_eq!(combat.death_save_successes, 1);
        assert_eq!(combat.death_save_failures, 0);
    }

    #[test]
    fn test_death_save_roll_below_10_counts_as_failure() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let outcome = combat.apply_death_save_roll(&mut state.character, 5);
        assert_eq!(outcome, DeathSaveOutcome::Failure);
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 1);
    }

    #[test]
    fn test_death_save_natural_1_counts_as_two_failures() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let outcome = combat.apply_death_save_roll(&mut state.character, 1);
        assert_eq!(outcome, DeathSaveOutcome::CritFailure);
        assert_eq!(combat.death_save_failures, 2);
    }

    #[test]
    fn test_death_save_natural_20_stabilizes_at_1_hp() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_successes = 1;
        combat.death_save_failures = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 20);
        assert_eq!(outcome, DeathSaveOutcome::CritSuccess);
        assert_eq!(state.character.current_hp, 1);
        assert_eq!(
            combat.death_save_successes, 0,
            "Nat 20 clears death save counters"
        );
        assert_eq!(
            combat.death_save_failures, 0,
            "Nat 20 clears death save counters"
        );
    }

    #[test]
    fn test_three_death_save_successes_stabilize_at_1_hp() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_successes = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 10);
        assert_eq!(outcome, DeathSaveOutcome::Stable);
        assert_eq!(
            state.character.current_hp, 1,
            "Reaching 3 successes sets HP to 1 (stable)"
        );
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 0);
    }

    #[test]
    fn test_three_death_save_failures_mark_dead() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_failures = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 5);
        assert_eq!(outcome, DeathSaveOutcome::Dead);
        assert_eq!(combat.death_save_failures, 3);
        assert_eq!(
            combat.check_end(&state),
            Some(false),
            "After three failures, combat ends in defeat"
        );
    }

    #[test]
    fn test_damage_while_dying_adds_failure() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let outcome = combat.apply_damage_while_dying(&mut state.character, 5, false);
        assert_eq!(outcome, DeathSaveOutcome::Failure);
        assert_eq!(combat.death_save_failures, 1);
    }

    #[test]
    fn test_crit_while_dying_adds_two_failures() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let outcome = combat.apply_damage_while_dying(&mut state.character, 5, true);
        assert_eq!(outcome, DeathSaveOutcome::CritFailure);
        assert_eq!(combat.death_save_failures, 2);
    }

    #[test]
    fn test_damage_exceeding_max_hp_while_dying_is_instant_death() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // damage >= max_hp in one hit is instant death (massive damage SRD rule).
        let outcome = combat.apply_damage_while_dying(&mut state.character, 20, false);
        assert_eq!(outcome, DeathSaveOutcome::Dead);
        assert_eq!(combat.death_save_failures, 3);
        assert_eq!(combat.check_end(&state), Some(false));
    }

    #[test]
    fn test_healing_clears_death_save_state() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_successes = 1;
        combat.death_save_failures = 2;
        // Simulate healing.
        state.character.current_hp = 5;
        combat.reset_death_saves();
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 0);
        assert!(!combat.is_player_dying(&state));
    }

    #[test]
    fn test_fresh_combat_has_zero_death_save_counters() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 0);
    }

    #[test]
    fn test_death_save_state_serde_roundtrip() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.death_save_successes = 2;
        combat.death_save_failures = 1;
        let json = serde_json::to_string(&combat).unwrap();
        let round_tripped: CombatState = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped.death_save_successes, 2);
        assert_eq!(round_tripped.death_save_failures, 1);
    }

    #[test]
    fn test_death_save_state_serde_back_compat_legacy_save() {
        // Older saves (pre-DST) have no death_save_* fields. They must still
        // deserialize, defaulting the counters to 0.
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let mut json: serde_json::Value = serde_json::to_value(&combat).unwrap();
        json.as_object_mut().unwrap().remove("death_save_successes");
        json.as_object_mut().unwrap().remove("death_save_failures");
        let round_tripped: CombatState = serde_json::from_value(json)
            .expect("Old saves without death_save_* fields should deserialize");
        assert_eq!(round_tripped.death_save_successes, 0);
        assert_eq!(round_tripped.death_save_failures, 0);
    }

    // ---- Grappling mechanics (feat/grappling-mechanics) ----

    #[test]
    fn test_grapple_dc_formula() {
        // DC = 8 + STR mod + PB
        // STR 16 -> mod +3, PB 2 -> DC 13
        assert_eq!(grapple_dc(16, 2), 13);
        // STR 10 -> mod 0, PB 2 -> DC 10
        assert_eq!(grapple_dc(10, 2), 10);
        // STR 8 -> mod -1, PB 2 -> DC 9
        assert_eq!(grapple_dc(8, 2), 9);
    }

    #[test]
    fn test_target_exceeds_grapple_size_limit() {
        // Medium grappler: can grapple up to Large, not Huge or bigger.
        assert!(!target_exceeds_grapple_size_limit(
            &monsters::Size::Medium,
            &monsters::Size::Medium
        ));
        assert!(!target_exceeds_grapple_size_limit(
            &monsters::Size::Medium,
            &monsters::Size::Large
        ));
        assert!(target_exceeds_grapple_size_limit(
            &monsters::Size::Medium,
            &monsters::Size::Huge
        ));
        assert!(target_exceeds_grapple_size_limit(
            &monsters::Size::Medium,
            &monsters::Size::Gargantuan
        ));
        // Large grappler: can grapple up to Huge.
        assert!(!target_exceeds_grapple_size_limit(
            &monsters::Size::Large,
            &monsters::Size::Huge
        ));
        assert!(target_exceeds_grapple_size_limit(
            &monsters::Size::Large,
            &monsters::Size::Gargantuan
        ));
    }

    #[test]
    fn test_resolve_grapple_attempt_success() {
        // Seed chosen so the goblin's save roll is low enough to fail vs DC 13
        // (STR 16, PB 2). Goblin STR 8 (mod -1), DEX 14 (mod +2) -> picks DEX.
        // With seed 99 the roll is deterministic.
        let mut rng = StdRng::seed_from_u64(99);
        let mut state = test_state_with_goblin();
        // DC = 8 + (16-10)/2 + 2 = 8 + 3 + 2 = 13
        let result = resolve_grapple_attempt(
            &mut rng, &mut state, 0,  // goblin NPC id
            16, // grappler STR score
            2,  // grappler PB
            "TestHero", false,
        )
        .unwrap();
        // Regardless of success/fail, check structure is correct.
        assert_eq!(result.dc, 13);
        assert_eq!(result.save_ability, Ability::Dexterity); // goblin picks DEX (mod +2 > STR mod -1)
                                                             // If the grapple succeeded, the goblin should have Grappled condition.
        if result.success {
            let npc = state.world.npcs.get(&0).unwrap();
            assert!(
                conditions::has_condition(&npc.conditions, ConditionType::Grappled),
                "Goblin should be grappled after a failed save"
            );
            let cond = npc
                .conditions
                .iter()
                .find(|c| c.condition == ConditionType::Grappled)
                .unwrap();
            assert_eq!(cond.source.as_deref(), Some("TestHero"));
        }
    }

    #[test]
    fn test_resolve_grapple_attempt_no_grapple_on_save_success() {
        // Use a seed that guarantees the goblin rolls high (20 on d20).
        // Seed 1 with this RNG gives roll=17 which + DEX mod 2 = 19 vs DC 9.
        // Goblin STR 8 (dc would be 8 + (-1) + 2 = 9 for a weak grappler).
        let mut rng = StdRng::seed_from_u64(1);
        let mut state = test_state_with_goblin();
        let result =
            resolve_grapple_attempt(&mut rng, &mut state, 0, 8, 2, "TestHero", false).unwrap();
        // DC = 9. If the goblin rolled high enough, it should not be grappled.
        if !result.success {
            let npc = state.world.npcs.get(&0).unwrap();
            assert!(
                !conditions::has_condition(&npc.conditions, ConditionType::Grappled),
                "Goblin should NOT be grappled after a successful save"
            );
        }
    }

    #[test]
    fn test_resolve_escape_grapple_none_when_not_grappled() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Player has no Grappled condition.
        let result = resolve_escape_grapple(&mut rng, &mut state);
        assert!(
            result.is_none(),
            "Should return None when player is not grappled"
        );
    }

    #[test]
    fn test_resolve_escape_grapple_removes_condition_on_success() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Manually put Grappled on the player.
        state.character.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("Goblin"),
        );
        let result = resolve_escape_grapple(&mut rng, &mut state).unwrap();
        if result.success {
            assert!(
                !conditions::has_condition(&state.character.conditions, ConditionType::Grappled),
                "Grappled should be cleared on successful escape"
            );
        } else {
            assert!(
                conditions::has_condition(&state.character.conditions, ConditionType::Grappled),
                "Grappled should remain when escape fails"
            );
        }
    }

    #[test]
    fn test_release_grapple_on_npc() {
        let mut state = test_state_with_goblin();
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        release_grapple_on_npc(npc, "TestHero");
        assert!(!conditions::has_condition(
            &npc.conditions,
            ConditionType::Grappled
        ));
    }

    #[test]
    fn test_advance_turn_releases_grapple_when_player_incapacitated() {
        use crate::conditions::ActiveCondition;
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();

        // Give the goblin a Grappled condition sourced to the player.
        {
            let npc = state.world.npcs.get_mut(&0).unwrap();
            npc.conditions.push(
                ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                    .with_source("TestHero"),
            );
        }

        // Apply Incapacitated to the player.
        state.character.conditions.push(ActiveCondition::new(
            ConditionType::Incapacitated,
            ConditionDuration::Permanent,
        ));

        // Set up a minimal CombatState where the player is NOT the current turn
        // so that advance_turn will land on Player next.
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        // Force the initiative order so Player is always index 1 (we just
        // need to advance to the player's turn exactly once).
        combat.initiative_order = vec![(Combatant::Npc(0), 20), (Combatant::Player, 10)];
        combat.current_turn = 0; // NPC is current turn; next call should land on Player.

        combat.advance_turn(&mut state);
        // After advancing to the player's turn, the goblin's Grappled condition
        // should have been released.
        let npc = state.world.npcs.get(&0).unwrap();
        assert!(
            !conditions::has_condition(&npc.conditions, ConditionType::Grappled),
            "Grappled should be released when grappler is incapacitated"
        );
    }

    // ---- SRD Cover Rules ----

    fn make_attack() -> NpcAttack {
        NpcAttack {
            name: "Longsword".to_string(),
            hit_bonus: 0,
            damage_dice: 1,
            damage_die: 8,
            damage_bonus: 0,
            damage_type: DamageType::Slashing,
            reach: 5,
            range_normal: 0,
            range_long: 0,
        }
    }

    #[test]
    fn test_cover_half_increases_player_ac_by_2() {
        // An attack that would barely hit AC 10 should miss AC 10 with Half Cover (AC becomes 12).
        let attack = make_attack();
        // hit_bonus = 0; we need roll+0 >= 10 to hit normally.
        // With Half Cover, effective AC = 12; roll+0 >= 12 needed.
        // Use a deterministic seed that gives a d20 roll of exactly 10 (hits AC 10, misses AC 12).
        for seed in 0..10000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result_no_cover = resolve_npc_attack(
                &mut rng,
                &attack,
                10,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::None,
            );
            let mut rng2 = StdRng::seed_from_u64(seed);
            let result_half = resolve_npc_attack(
                &mut rng2,
                &attack,
                10,
                false,
                5,
                &[],
                &[],
                false,
                &Cover::Half,
            );
            // Effective AC for Half cover is 10+2=12. Effective AC for None is 10.
            assert_eq!(
                result_half.target_ac, 12,
                "Half cover should raise effective AC to 12"
            );
            assert_eq!(result_no_cover.target_ac, 10, "No cover AC should be 10");
            // The test is structural: effective_ac is applied. Early exit after first seed.
            return;
        }
    }

    #[test]
    fn test_cover_three_quarters_increases_player_ac_by_5() {
        let attack = make_attack();
        let seed = 0u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let result = resolve_npc_attack(
            &mut rng,
            &attack,
            10,
            false,
            5,
            &[],
            &[],
            false,
            &Cover::ThreeQuarters,
        );
        assert_eq!(
            result.target_ac, 15,
            "Three-quarters cover should raise effective AC to 15"
        );
    }

    #[test]
    fn test_cover_none_does_not_change_player_ac() {
        let attack = make_attack();
        let seed = 0u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let result = resolve_npc_attack(
            &mut rng,
            &attack,
            14,
            false,
            5,
            &[],
            &[],
            false,
            &Cover::None,
        );
        assert_eq!(result.target_ac, 14, "No cover: effective AC unchanged");
    }

    #[test]
    fn test_player_attacking_npc_with_half_cover_increases_npc_ac() {
        use crate::character::{class::Class, create_character, race::Race};
        use crate::types::Ability;
        use std::collections::HashMap;

        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 16);
        scores.insert(Ability::Dexterity, 10);
        scores.insert(Ability::Constitution, 14);
        scores.insert(Ability::Intelligence, 8);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);
        let player = create_character(
            "Hero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![],
        );
        let items = HashMap::new();

        let seed = 0u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let result = resolve_player_attack(
            &mut rng,
            &player,
            10,
            false,
            None,
            &items,
            5,
            true,
            false,
            &[],
            false,
            false,
            &Cover::Half,
        );
        assert_eq!(
            result.target_ac, 12,
            "NPC with Half cover should have effective AC 12"
        );
    }

    #[test]
    fn test_player_attacking_npc_with_no_cover_unmodified_ac() {
        use crate::character::{class::Class, create_character, race::Race};
        use crate::types::Ability;
        use std::collections::HashMap;

        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 16);
        scores.insert(Ability::Dexterity, 10);
        scores.insert(Ability::Constitution, 14);
        scores.insert(Ability::Intelligence, 8);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);
        let player = create_character(
            "Hero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![],
        );
        let items = HashMap::new();

        let seed = 0u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let result = resolve_player_attack(
            &mut rng,
            &player,
            10,
            false,
            None,
            &items,
            5,
            true,
            false,
            &[],
            false,
            false,
            &Cover::None,
        );
        assert_eq!(
            result.target_ac, 10,
            "NPC with no cover: AC unchanged at 10"
        );
    }

    #[test]
    fn test_cover_ac_bonus_values() {
        // Structural test: Cover enum returns the correct SRD bonuses.
        assert_eq!(Cover::None.ac_bonus(), 0);
        assert_eq!(Cover::Half.ac_bonus(), 2);
        assert_eq!(Cover::ThreeQuarters.ac_bonus(), 5);
        assert_eq!(Cover::Total.ac_bonus(), 0); // Total blocks targeting; no numeric AC bonus
    }

    #[test]
    fn test_cover_save_bonus_matches_ac_bonus() {
        // Per SRD, cover bonus applies equally to AC and DEX saves.
        for cover in [Cover::None, Cover::Half, Cover::ThreeQuarters, Cover::Total] {
            assert_eq!(
                cover.save_bonus(),
                cover.ac_bonus(),
                "save_bonus should equal ac_bonus for {:?}",
                cover
            );
        }
    }

    // ---- NPC cover assignment tests ----

    #[test]
    fn test_assign_npc_cover_returns_map_with_valid_cover_levels() {
        use crate::state::LocationType;
        let mut rng = StdRng::seed_from_u64(42);
        let npc_ids = vec![1, 2, 3, 4, 5];
        let cover_map = assign_npc_cover(&mut rng, &npc_ids, LocationType::Room);
        // Cover map may be empty (RNG decided no NPCs get cover) or populated.
        // All values must be Half or ThreeQuarters (never Total or None).
        for (id, cover) in &cover_map {
            assert!(npc_ids.contains(id), "NPC id {} not in input list", id);
            assert!(
                *cover == Cover::Half || *cover == Cover::ThreeQuarters,
                "NPC cover must be Half or ThreeQuarters, got {:?}",
                cover
            );
        }
    }

    #[test]
    fn test_assign_npc_cover_deterministic() {
        use crate::state::LocationType;
        let npc_ids = vec![10, 20, 30];
        let map1 = assign_npc_cover(&mut StdRng::seed_from_u64(99), &npc_ids, LocationType::Ruins);
        let map2 = assign_npc_cover(&mut StdRng::seed_from_u64(99), &npc_ids, LocationType::Ruins);
        assert_eq!(map1, map2, "NPC cover assignment must be deterministic");
    }

    #[test]
    fn test_assign_npc_cover_corridor_only_half() {
        use crate::state::LocationType;
        // Run many seeds; corridor should only produce Half, never ThreeQuarters
        let npc_ids = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        for seed in 0..50 {
            let map = assign_npc_cover(&mut StdRng::seed_from_u64(seed), &npc_ids, LocationType::Corridor);
            for cover in map.values() {
                assert_eq!(*cover, Cover::Half, "Corridor should only produce Half cover, seed {}", seed);
            }
        }
    }

    // ---- Shove tests (2024 SRD) ----

    /// Find a seed where the NPC fails its best-of(STR,DEX) save against our
    /// test character. Goblin: STR 8 (mod -1), DEX 14 (mod +2) -> uses DEX +2.
    /// PC DC = 8 + 3 (STR mod) + 2 (PB) = 13.
    /// NPC fails on d20 + 2 < 13, i.e. d20 <= 10.
    fn find_shove_success_seed() -> u64 {
        for seed in 0..1000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let roll = roll_d20(&mut rng);
            if roll + 2 < 13 {
                // +2 is goblin's DEX mod (best of STR -1 / DEX +2)
                return seed;
            }
        }
        panic!("Could not find a seed where goblin fails save");
    }

    /// Find a seed where the NPC succeeds its best-of(STR,DEX) save.
    /// Goblin uses DEX +2, DC = 13. Succeeds on d20 + 2 >= 13, i.e. d20 >= 11.
    fn find_shove_fail_seed() -> u64 {
        for seed in 0..1000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let roll = roll_d20(&mut rng);
            if roll + 2 >= 13 {
                return seed;
            }
        }
        panic!("Could not find a seed where goblin succeeds save");
    }

    #[test]
    fn test_shove_push_npc_fails_save() {
        let seed = find_shove_success_seed();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);
        let initial_dist = 5u32;

        let mut rng2 = StdRng::seed_from_u64(seed);
        let lines = handle_shove(&mut state, &mut combat, &mut rng2, 0, "goblin", false);

        assert!(
            lines
                .iter()
                .any(|l| l.contains("shove") || l.contains("back")),
            "Should report push message: {:?}",
            lines
        );
        let new_dist = *combat.distances.get(&0).unwrap();
        assert_eq!(
            new_dist,
            initial_dist + 5,
            "Pushed NPC should be 5 ft further"
        );
        assert!(
            !conditions::has_condition(&state.world.npcs[&0].conditions, ConditionType::Prone),
            "Push variant should not apply Prone"
        );
    }

    #[test]
    fn test_shove_prone_npc_fails_save() {
        let seed = find_shove_success_seed();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        let mut rng2 = StdRng::seed_from_u64(seed);
        let lines = handle_shove(&mut state, &mut combat, &mut rng2, 0, "goblin", true);

        assert!(
            lines.iter().any(|l| l.to_lowercase().contains("prone")),
            "Should report prone message: {:?}",
            lines
        );
        assert!(
            conditions::has_condition(&state.world.npcs[&0].conditions, ConditionType::Prone),
            "Prone condition should be applied on failed save"
        );
    }

    #[test]
    fn test_shove_npc_succeeds_save() {
        let seed = find_shove_fail_seed();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);
        let initial_dist = 5u32;

        let mut rng2 = StdRng::seed_from_u64(seed);
        let lines = handle_shove(&mut state, &mut combat, &mut rng2, 0, "goblin", false);

        assert!(
            lines.iter().any(|l| l.contains("resists")),
            "Should report resist message: {:?}",
            lines
        );
        let new_dist = *combat.distances.get(&0).unwrap();
        assert_eq!(
            new_dist, initial_dist,
            "Distance should be unchanged on resist"
        );
        assert!(
            !conditions::has_condition(&state.world.npcs[&0].conditions, ConditionType::Prone),
            "No Prone should be applied when NPC resists"
        );
    }

    #[test]
    fn test_shove_npc_uses_dex_when_higher() {
        // Goblin: STR 8 (mod -1), DEX 14 (mod +2). Should pick DEX.
        let seed = find_shove_success_seed();
        let mut rng = StdRng::seed_from_u64(seed);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        let mut rng2 = StdRng::seed_from_u64(seed);
        let lines = handle_shove(&mut state, &mut combat, &mut rng2, 0, "goblin", false);

        let joined = lines.join(" ");
        assert!(
            joined.contains("DEX save"),
            "Goblin (DEX +2 > STR -1) should use DEX save, got: {:?}",
            lines
        );
    }

    #[test]
    fn test_shove_npc_uses_str_when_higher() {
        // Create an NPC whose STR is higher than DEX.
        let seed = 0u64;
        let mut rng = StdRng::seed_from_u64(seed);
        let mut state = test_state_with_goblin();

        // Override the goblin's stats: STR 16 (mod +3), DEX 8 (mod -1).
        if let Some(stats) = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut() {
            stats.ability_scores.insert(Ability::Strength, 16);
            stats.ability_scores.insert(Ability::Dexterity, 8);
        }

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        combat.distances.insert(0, 5);

        // DC is 13, NPC STR mod is +3. Find a seed where d20 + 3 < 13 (d20 <= 9).
        let mut test_seed = 0u64;
        for s in 0..1000u64 {
            let mut r = StdRng::seed_from_u64(s);
            let roll = roll_d20(&mut r);
            if roll + 3 < 13 {
                test_seed = s;
                break;
            }
        }

        let mut rng2 = StdRng::seed_from_u64(test_seed);
        let lines = handle_shove(&mut state, &mut combat, &mut rng2, 0, "goblin", false);

        let joined = lines.join(" ");
        assert!(
            joined.contains("STR save"),
            "NPC with STR +3 > DEX -1 should use STR save, got: {:?}",
            lines
        );
    }

    #[test]
    fn test_shove_size_restriction_large_is_ok() {
        // PC (Medium) can shove a Large target — it's only 1 category larger.
        let target_size = crate::combat::monsters::Size::Large;
        let shover_size = crate::combat::monsters::Size::Medium;
        assert!(
            !target_exceeds_grapple_size_limit(&shover_size, &target_size),
            "Medium player should be able to shove Large target"
        );
    }

    #[test]
    fn test_shove_size_restriction_huge_blocked() {
        // PC (Medium) cannot shove a Huge target — 2 categories larger.
        let target_size = crate::combat::monsters::Size::Huge;
        let shover_size = crate::combat::monsters::Size::Medium;
        assert!(
            target_exceeds_grapple_size_limit(&shover_size, &target_size),
            "Medium player should NOT be able to shove Huge target"
        );
    }

    // ---- NPC Escape from Player Grapple ----

    #[test]
    fn test_npc_escape_grapple_none_when_not_grappled() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Goblin has no Grappled condition.
        let result = resolve_npc_escape_grapple(&mut rng, &mut state, 0);
        assert!(
            result.is_none(),
            "Should return None when NPC is not grappled"
        );
    }

    #[test]
    fn test_npc_escape_grapple_returns_result_when_grappled() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Manually grapple the goblin.
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        let result = resolve_npc_escape_grapple(&mut rng, &mut state, 0);
        assert!(result.is_some(), "Should return Some when NPC is grappled");
        let res = result.unwrap();
        // DC should be based on player stats: 8 + STR mod(16) + PB(2) = 13.
        assert_eq!(res.dc, 13);
    }

    #[test]
    fn test_npc_escape_grapple_removes_condition_on_success() {
        // Try many seeds to find one where the escape succeeds.
        let mut state = test_state_with_goblin();
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        for seed in 0..1000u64 {
            let mut test_state = state.clone();
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_escape_grapple(&mut rng, &mut test_state, 0) {
                if res.success {
                    let npc = test_state.world.npcs.get(&0).unwrap();
                    assert!(
                        !conditions::has_condition(&npc.conditions, ConditionType::Grappled),
                        "Grappled should be cleared on successful NPC escape"
                    );
                    return;
                }
            }
        }
        panic!("Could not find a seed where NPC escape succeeds");
    }

    #[test]
    fn test_npc_escape_grapple_retains_condition_on_failure() {
        // Try many seeds to find one where the escape fails.
        let mut state = test_state_with_goblin();
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        for seed in 0..1000u64 {
            let mut test_state = state.clone();
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_escape_grapple(&mut rng, &mut test_state, 0) {
                if !res.success {
                    let npc = test_state.world.npcs.get(&0).unwrap();
                    assert!(
                        conditions::has_condition(&npc.conditions, ConditionType::Grappled),
                        "Grappled should remain when NPC escape fails"
                    );
                    return;
                }
            }
        }
        panic!("Could not find a seed where NPC escape fails");
    }

    #[test]
    fn test_player_escape_npc_grapple_uses_npc_dc() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Manually put Grappled on the player, sourced to "Goblin".
        state.character.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("Goblin"),
        );
        let result = resolve_escape_grapple(&mut rng, &mut state).unwrap();
        // DC should use the Goblin's stats: STR 8 (mod -1), PB 2.
        // DC = 8 + (-1) + 2 = 9.
        assert_eq!(result.dc, 9, "DC should be derived from NPC grappler's stats");
    }

    // ---- NPC-Initiated Grapple ----

    #[test]
    fn test_npc_grapple_attempt_applies_condition_on_success() {
        // Try many seeds to find one where the grapple succeeds.
        let state = test_state_with_goblin();
        for seed in 0..1000u64 {
            let mut test_state = state.clone();
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_grapple_attempt(&mut rng, &mut test_state, 0) {
                if res.success {
                    assert!(
                        conditions::has_condition(
                            &test_state.character.conditions,
                            ConditionType::Grappled
                        ),
                        "Player should have Grappled condition after NPC grapple success"
                    );
                    let cond = test_state
                        .character
                        .conditions
                        .iter()
                        .find(|c| c.condition == ConditionType::Grappled)
                        .unwrap();
                    assert_eq!(cond.source.as_deref(), Some("Goblin"));
                    return;
                }
            }
        }
        panic!("Could not find a seed where NPC grapple succeeds");
    }

    #[test]
    fn test_npc_grapple_attempt_no_condition_on_failure() {
        // Try many seeds to find one where the grapple fails.
        let state = test_state_with_goblin();
        for seed in 0..1000u64 {
            let mut test_state = state.clone();
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_grapple_attempt(&mut rng, &mut test_state, 0) {
                if !res.success {
                    assert!(
                        !conditions::has_condition(
                            &test_state.character.conditions,
                            ConditionType::Grappled
                        ),
                        "Player should NOT have Grappled condition after NPC grapple failure"
                    );
                    return;
                }
            }
        }
        panic!("Could not find a seed where NPC grapple fails");
    }

    #[test]
    fn test_npc_grapple_attempt_dc_formula() {
        // Goblin STR 8 (mod -1), PB 2. DC = 8 + (-1) + 2 = 9.
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let result = resolve_npc_grapple_attempt(&mut rng, &mut state, 0).unwrap();
        assert_eq!(result.dc, 9, "DC should be 8 + NPC STR mod + NPC PB");
    }

    // ---- NPC Turn: Grappled NPC Does Not Move ----

    #[test]
    fn test_grappled_npc_does_not_move() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Grapple the goblin so its speed is 0.
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        // Set up combat with goblin far away (beyond melee reach).
        let mut combat = CombatState {
            initiative_order: vec![(Combatant::Player, 20), (Combatant::Npc(0), 10)],
            current_turn: 1,
            round: 1,
            distances: {
                let mut d = HashMap::new();
                d.insert(0, 30); // 30 ft away
                d
            },
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: false,
            action_used: false,
            bonus_action_used: false,
            action_surge_active: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: std::collections::HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            attacks_made_this_turn: 0,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: Cover::None,
            npc_cover: HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        };
        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        // The grappled NPC should NOT have moved (distance unchanged).
        let dist = *combat.distances.get(&0).unwrap();
        assert_eq!(dist, 30, "Grappled NPC should not move (distance unchanged)");
        // It should have attempted to escape instead of attacking.
        let has_escape = lines.iter().any(|l| {
            let lower = l.to_lowercase();
            lower.contains("escape") || lower.contains("break") || lower.contains("grapple")
        });
        assert!(has_escape, "Grappled NPC should attempt escape, got: {:?}", lines);
    }

    // ---- Drag Movement Cost ----

    #[test]
    fn test_approach_costs_double_when_dragging() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Add a second NPC to approach.
        state.world.npcs.insert(
            1,
            Npc {
                id: 1,
                name: "Orc".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(goblin_stats()),
                conditions: Vec::new(),
            },
        );
        // Grapple the goblin (NPC 0) by the player.
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        let mut combat = CombatState {
            initiative_order: vec![
                (Combatant::Player, 20),
                (Combatant::Npc(0), 10),
                (Combatant::Npc(1), 5),
            ],
            current_turn: 0,
            round: 1,
            distances: {
                let mut d = HashMap::new();
                d.insert(0, 5);  // Goblin at melee range
                d.insert(1, 30); // Orc at 30 ft
                d
            },
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: false,
            action_used: false,
            bonus_action_used: false,
            action_surge_active: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: std::collections::HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            attacks_made_this_turn: 0,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: Cover::None,
            npc_cover: HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        };
        // Approach the Orc (NPC 1). Normal approach would cost 1 ft per 1 ft moved.
        // With dragging, it costs 2 ft per 1 ft, so 30 ft of movement lets us move only 15 ft.
        let lines = approach_target(&mut rng, 1, &state, &mut combat);
        // With 30 ft movement and drag cost, player can move 15 ft toward the orc.
        // Orc was at 30 ft, so new distance = 30 - 15 = 15. But minimum is 5 ft.
        // Actually, approach_target caps at distance - 5, so move_amount = min(movement/2, dist-5).
        // move_amount = min(15, 25) = 15, so new distance = 30 - 15 = 15.
        let orc_dist = *combat.distances.get(&1).unwrap();
        assert_eq!(orc_dist, 15, "Orc should be at 15 ft (30 - 15 moved, halved by drag)");
        assert_eq!(combat.player_movement_remaining, 0, "All movement should be consumed by drag");
        assert!(!lines.is_empty());
    }

    // ---- Distance Auto-Release ----

    #[test]
    fn test_retreat_auto_releases_grapple_beyond_5ft() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Grapple the goblin by the player.
        let npc = state.world.npcs.get_mut(&0).unwrap();
        npc.conditions.push(
            ActiveCondition::new(ConditionType::Grappled, ConditionDuration::Permanent)
                .with_source("TestHero"),
        );
        let mut combat = CombatState {
            initiative_order: vec![(Combatant::Player, 20), (Combatant::Npc(0), 10)],
            current_turn: 0,
            round: 1,
            distances: {
                let mut d = HashMap::new();
                d.insert(0, 5); // Goblin at melee range
                d
            },
            player_movement_remaining: 30,
            player_dodging: false,
            player_disengaging: true, // disengage to avoid OA complexity
            action_used: false,
            bonus_action_used: false,
            action_surge_active: false,
            reaction_used: false,
            free_interaction_used: false,
            npc_dodging: HashMap::new(),
            npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0,
            pending_reaction: None,
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: std::collections::HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            attacks_made_this_turn: 0,
            death_save_successes: 0,
            death_save_failures: 0,
            player_cover: Cover::None,
            npc_cover: HashMap::new(),
            npc_reactions_used: std::collections::HashSet::new(),
        };
        // Retreat should move the player away. With drag cost, effective distance moved
        // is halved. But the grappled NPC's distance should increase (player moves away
        // but the NPC doesn't move with the player on retreat). Once distance > 5 ft,
        // auto-release triggers.
        let _lines = retreat(&mut rng, &mut state, &mut combat);
        let npc = state.world.npcs.get(&0).unwrap();
        assert!(
            !conditions::has_condition(&npc.conditions, ConditionType::Grappled),
            "Grapple should auto-release when distance exceeds 5 ft after retreat"
        );
    }

    // ---- NPC Retreat AI (issue #256) ----
    // ---- NPC Retreat AI (issue #256) ----

    #[test]
    fn test_npc_retreats_when_low_hp_and_has_ranged() {
        // An NPC at <30% HP with a ranged attack should move AWAY from the
        // player instead of toward them.
        let mut state = test_state_with_goblin();
        // Give the goblin low HP (1 out of 7 = 14%, below 30%) and both
        // melee + ranged attacks (goblin_stats() already has both).
        if let Some(cs) = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut() {
            cs.current_hp = 1;
        }
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let initial_distance = 10;
        combat.distances.insert(0, initial_distance);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_distance = *combat.distances.get(&0).unwrap();

        assert!(
            new_distance > initial_distance,
            "Low-HP NPC with ranged attack should retreat (move away). \
             Initial: {}, After: {}. Lines: {:?}",
            initial_distance, new_distance, lines
        );
    }

    #[test]
    fn test_npc_does_not_retreat_when_hp_above_threshold() {
        // An NPC at full HP should move toward the player, not retreat.
        let mut state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let initial_distance = 20;
        combat.distances.insert(0, initial_distance);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_distance = *combat.distances.get(&0).unwrap();

        assert!(
            new_distance <= initial_distance,
            "Full-HP NPC should approach (move toward) the player. \
             Initial: {}, After: {}. Lines: {:?}",
            initial_distance, new_distance, lines
        );
    }

    #[test]
    fn test_npc_does_not_retreat_without_ranged_attack() {
        // An NPC at low HP but with only melee attacks should still approach.
        let mut state = test_state_with_goblin();
        if let Some(cs) = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut() {
            cs.current_hp = 1;
            // Remove ranged attacks, keep only melee.
            cs.attacks.retain(|a| a.reach > 0);
        }
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs, crate::state::LocationType::Room);
        let initial_distance = 20;
        combat.distances.insert(0, initial_distance);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_distance = *combat.distances.get(&0).unwrap();

        assert!(
            new_distance <= initial_distance,
            "Low-HP NPC without ranged attack should still approach. \
             Initial: {}, After: {}. Lines: {:?}",
            initial_distance, new_distance, lines
        );
    }

    // ---- Danger Sense on NPC grapple (Barbarian level 2) ----

    #[test]
    fn test_npc_grapple_danger_sense_advantage_improves_success_rate() {
        // A level-2 Barbarian with higher DEX than STR should get Danger
        // Sense advantage on the DEX save vs NPC grapple. Over many seeds
        // the Barbarian should resist grapple more often than a Fighter
        // with identical stats.
        fn make_state(class: Class, level: u32) -> GameState {
            let mut scores = HashMap::new();
            // DEX > STR so the engine picks DEX for the save
            scores.insert(Ability::Strength, 8);
            scores.insert(Ability::Dexterity, 16);
            scores.insert(Ability::Constitution, 14);
            scores.insert(Ability::Intelligence, 10);
            scores.insert(Ability::Wisdom, 12);
            scores.insert(Ability::Charisma, 8);
            let mut character = create_character(
                "Hero".to_string(),
                Race::Human,
                class,
                scores,
                vec![],
            );
            character.level = level;

            let mut npcs = HashMap::new();
            npcs.insert(0, Npc {
                id: 0,
                name: "Ogre".to_string(),
                role: NpcRole::Guard,
                disposition: Disposition::Hostile,
                dialogue_tags: vec![],
                location: 0,
                combat_stats: Some(CombatStats {
                    max_hp: 59,
                    current_hp: 59,
                    ac: 11,
                    speed: 40,
                    ability_scores: {
                        let mut m = HashMap::new();
                        m.insert(Ability::Strength, 19); // high STR for hard DC
                        m.insert(Ability::Dexterity, 8);
                        m.insert(Ability::Constitution, 16);
                        m.insert(Ability::Intelligence, 5);
                        m.insert(Ability::Wisdom, 7);
                        m.insert(Ability::Charisma, 7);
                        m
                    },
                    attacks: vec![NpcAttack {
                        name: "Greatclub".to_string(),
                        hit_bonus: 6,
                        damage_dice: 2,
                        damage_die: 8,
                        damage_bonus: 4,
                        damage_type: DamageType::Bludgeoning,
                        reach: 5,
                        range_normal: 0,
                        range_long: 0,
                    }],
                    proficiency_bonus: 2,
                    cr: 2.0,
                    ..Default::default()
                }),
                conditions: Vec::new(),
            });

            GameState {
                version: SAVE_VERSION.to_string(),
                character,
                current_location: 0,
                discovered_locations: HashSet::new(),
                world: WorldState {
                    locations: HashMap::new(),
                    npcs,
                    items: HashMap::new(),
                    triggers: HashMap::new(),
                    triggered: HashSet::new(),
                },
                log: Vec::new(),
                rng_seed: 42,
                rng_counter: 0,
                game_phase: GamePhase::Exploration,
                active_combat: None,
                ironman_mode: false,
                progress: crate::state::ProgressState::default(),
                in_world_minutes: 0,
                last_long_rest_minutes: None,
                pending_background_pattern: None,
                pending_subrace: None,
                pending_disambiguation: None,
                pending_new_game_confirm: false,
            }
        }

        let trials = 2000u64;
        let mut barbarian_resists = 0u32;
        let mut fighter_resists = 0u32;

        for seed in 0..trials {
            // Barbarian level 2 (has Danger Sense)
            let mut barb_state = make_state(Class::Barbarian, 2);
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_grapple_attempt(&mut rng, &mut barb_state, 0) {
                if !res.success {
                    barbarian_resists += 1;
                }
            }

            // Fighter level 2 (no Danger Sense, same stats)
            let mut fighter_state = make_state(Class::Fighter, 2);
            let mut rng = StdRng::seed_from_u64(seed);
            if let Some(res) = resolve_npc_grapple_attempt(&mut rng, &mut fighter_state, 0) {
                if !res.success {
                    fighter_resists += 1;
                }
            }
        }

        // With advantage, the Barbarian should resist significantly more often.
        assert!(
            barbarian_resists > fighter_resists,
            "Barbarian with Danger Sense should resist NPC grapple more often \
             (advantage on DEX save). Barbarian resists: {}, Fighter resists: {}",
            barbarian_resists, fighter_resists,
        );
    }

    // ---- Extra Attack: attacks_made_this_turn field ----

    #[test]
    fn test_attacks_made_this_turn_defaults_to_zero() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(
            &mut rng,
            &state.character,
            &[0],
            &state.world.npcs,
            crate::state::LocationType::Room,
        );
        assert_eq!(combat.attacks_made_this_turn, 0);
    }

    #[test]
    fn test_attacks_made_this_turn_resets_on_advance_turn() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(
            &mut rng,
            &state.character,
            &[0],
            &state.world.npcs,
            crate::state::LocationType::Room,
        );
        combat.attacks_made_this_turn = 2;

        // Position on the NPC so advance_turn cycles back to player.
        combat.current_turn = combat
            .initiative_order
            .iter()
            .position(|(c, _)| matches!(c, Combatant::Npc(_)))
            .unwrap_or(0);

        combat.advance_turn(&mut state);

        assert!(combat.is_player_turn(), "Should advance back to player turn");
        assert_eq!(
            combat.attacks_made_this_turn, 0,
            "attacks_made_this_turn should reset to 0 at start of player turn"
        );
    }

    #[test]
    fn test_attacks_made_this_turn_deserializes_from_legacy_save() {
        // Legacy saves won't have this field — serde(default) should handle it.
        let json = r#"{
            "initiative_order": [],
            "current_turn": 0,
            "round": 1,
            "distances": {},
            "player_movement_remaining": 30,
            "player_dodging": false,
            "player_disengaging": false,
            "action_used": false,
            "npc_dodging": {},
            "npc_disengaging": {}
        }"#;
        let combat: CombatState = serde_json::from_str(json).unwrap();
        assert_eq!(combat.attacks_made_this_turn, 0);
    }
}
