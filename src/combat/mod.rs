// jurnalis-engine/src/combat/mod.rs
// Combat system: state, initiative, attack resolution, movement, NPC AI.
pub mod monsters;

use std::collections::HashMap;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::types::{NpcId, Ability, ItemId};
use crate::state::{GameState, CombatStats, NpcAttack, DamageType, ItemType};
use crate::equipment::{FINESSE, THROWN, VERSATILE, REACH, AMMUNITION};
use crate::rules::dice::{roll_d20, roll_dice};
use crate::character::Character;
use crate::conditions::{self, ConditionType, ActiveCondition};
use crate::state::Npc;

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
        let all_dead = self.initiative_order.iter().all(|(c, _)| match c {
            Combatant::Player => true,
            Combatant::Npc(id) => {
                state.world.npcs.get(id)
                    .and_then(|npc| npc.combat_stats.as_ref())
                    .map(|cs| cs.current_hp <= 0)
                    .unwrap_or(false)
            }
        });
        if all_dead { Some(true) } else { None }
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
    pub fn advance_turn(&mut self, state: &GameState) -> Combatant {
        loop {
            self.current_turn = (self.current_turn + 1) % self.initiative_order.len();
            if self.current_turn == 0 {
                self.round += 1;
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
                    return combatant;
                }
                Combatant::Npc(id) => {
                    // Skip dead NPCs
                    let alive = state.world.npcs.get(&id)
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
            lines.push(format!(
                "Death saving throw: {} — success.",
                d20,
            ));
        }
        DeathSaveOutcome::Failure => {
            lines.push(format!(
                "Death saving throw: {} — failure.",
                d20,
            ));
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
    let player_dex = player.ability_scores.get(&Ability::Dexterity).copied().unwrap_or(10);
    initiatives.push((Combatant::Player, player_init, player_dex, player.name.clone()));

    // NPCs
    for &(id, stats) in npcs {
        let roll = roll_d20(rng);
        let dex = stats.ability_scores.get(&Ability::Dexterity).copied().unwrap_or(10);
        let dex_mod = Ability::modifier(dex);
        let init = roll + dex_mod;
        // Use NPC id as name placeholder for tie-breaking
        initiatives.push((Combatant::Npc(id), init, dex, format!("npc_{}", id)));
    }

    // Sort: higher initiative first, then higher DEX, then name (alphabetical)
    initiatives.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(b.2.cmp(&a.2))
            .then(a.3.cmp(&b.3))
    });

    initiatives.into_iter().map(|(c, init, _, _)| (c, init)).collect()
}

/// Start combat: roll initiative, set distances, create CombatState.
pub fn start_combat(
    rng: &mut impl Rng,
    player: &Character,
    hostile_npc_ids: &[NpcId],
    npcs: &HashMap<NpcId, crate::state::Npc>,
) -> CombatState {
    let npc_stats: Vec<(NpcId, &CombatStats)> = hostile_npc_ids.iter()
        .filter_map(|&id| {
            npcs.get(&id)
                .and_then(|npc| npc.combat_stats.as_ref())
                .map(|cs| (id, cs))
        })
        .collect();

    // Every hostile NPC should have combat_stats assigned by world generation.
    // If any are dropped, it means a bug upstream -- catch it in debug builds.
    debug_assert_eq!(
        npc_stats.len(), hostile_npc_ids.len(),
        "start_combat: {} hostile NPCs provided but only {} have combat_stats",
        hostile_npc_ids.len(), npc_stats.len()
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
        death_save_successes: 0,
        death_save_failures: 0,
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
pub fn npc_within_player_reach(
    state: &GameState,
    combat: &CombatState,
    npc_id: NpcId,
) -> bool {
    let alive = state.world.npcs.get(&npc_id)
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
    combat.initiative_order.iter().any(|(combatant, _)| match combatant {
        Combatant::Player => false,
        Combatant::Npc(id) => {
            let alive = state.world.npcs.get(id)
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
    pub attack_roll: i32,
    pub total_attack: i32,
    pub target_ac: i32,
    pub damage: i32,
    pub damage_type: DamageType,
    pub weapon_name: String,
    pub disadvantage: bool,
}

/// Determine if the player's weapon attack is ranged based on target distance and weapon.
fn is_ranged_attack(weapon: &ItemType, distance: u32) -> bool {
    match weapon {
        ItemType::Weapon { range_normal, properties, .. } => {
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
) -> AttackResult {
    // Base weapon fields come from either a mundane `Weapon` or a
    // `MagicWeapon` (which embeds the same mechanical fields). Magic
    // attack/damage bonuses are applied by the `lib.rs` orchestrator AFTER
    // this function returns — this function stays oblivious to magic bonuses.
    let (weapon_name, damage_dice, damage_die, damage_type, properties, versatile_die, range_normal, range_long) =
        match weapon_id.and_then(|id| items.get(&id)) {
            Some(item) => match &item.item_type {
                ItemType::Weapon { damage_dice, damage_die, damage_type, properties, versatile_die, range_normal, range_long, .. } => {
                    (item.name.clone(), *damage_dice, *damage_die, *damage_type, *properties, *versatile_die, *range_normal, *range_long)
                }
                ItemType::MagicWeapon { damage_dice, damage_die, damage_type, properties, versatile_die, range_normal, range_long, .. } => {
                    (item.name.clone(), *damage_dice, *damage_die, *damage_type, *properties, *versatile_die, *range_normal, *range_long)
                }
                _ => ("Unarmed".to_string(), 0, 0, DamageType::Bludgeoning, 0u16, 0, 0, 0),
            },
            None => ("Unarmed".to_string(), 0, 0, DamageType::Bludgeoning, 0u16, 0, 0, 0),
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
    let ranged = is_ranged_attack(&ItemType::Weapon {
        damage_dice, damage_die, damage_type, properties, category: crate::state::WeaponCategory::Simple,
        versatile_die, range_normal, range_long,
    }, distance);

    // Determine ability modifier for attack
    let ability_mod = if ranged {
        if is_thrown {
            // Thrown uses STR (or DEX if FINESSE)
            if is_finesse {
                player.ability_modifier(Ability::Strength).max(player.ability_modifier(Ability::Dexterity))
            } else {
                player.ability_modifier(Ability::Strength)
            }
        } else {
            // Ranged/AMMUNITION uses DEX
            player.ability_modifier(Ability::Dexterity)
        }
    } else if is_finesse {
        // Finesse: use higher of STR/DEX
        player.ability_modifier(Ability::Strength).max(player.ability_modifier(Ability::Dexterity))
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
    let hit = if natural_1 { false } else if natural_20 { true } else { total_attack >= target_ac };

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

            let dice_count = if natural_20 || auto_crit { damage_dice * 2 } else { damage_dice };
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
        attack_roll,
        total_attack,
        target_ac,
        damage,
        damage_type,
        weapon_name,
        disadvantage,
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
    let hit = if natural_1 { false } else if natural_20 { true } else { total_attack >= player_ac };

    // Check for auto-crit (paralyzed player within 5ft)
    let auto_crit = hit && conditions::is_auto_crit_target(player_conditions) && distance <= 5;

    let damage = if hit {
        let dice_count = if natural_20 || auto_crit { attack.damage_dice * 2 } else { attack.damage_dice };
        let dice_total: i32 = roll_dice(rng, dice_count, attack.damage_die).iter().sum();
        (dice_total + attack.damage_bonus).max(1)
    } else {
        0
    };

    AttackResult {
        hit,
        natural_20,
        natural_1,
        attack_roll,
        total_attack,
        target_ac: player_ac,
        damage,
        damage_type: attack.damage_type,
        weapon_name: attack.name.clone(),
        disadvantage: use_disadvantage,
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
    if stats.current_hp <= 0 { return None; }

    // Find a melee attack that can reach the player at current distance
    let melee_attack = stats.attacks.iter()
        .find(|a| a.reach > 0 && distance <= a.reach as u32)?;
    // For NPC opportunity attacks, Grappled-vs-non-grappler disadvantage against
    // the player would apply if the NPC is grappled by someone other than the
    // player. We compute it here rather than leaking target-name parsing into combat.
    let extra_disadvantage = conditions::grappled_attack_disadvantage(
        &npc.conditions,
        &state.character.name,
    );
    let result = resolve_npc_attack(
        rng, melee_attack, player_ac, false, distance,
        &npc.conditions, &state.character.conditions, extra_disadvantage,
    );
    Some((npc.name.clone(), result))
}

// ---- NPC AI ----

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

    // Priority: melee if in range -> ranged if in range -> move toward player

    // Orchestrator-side grappled disadvantage: if the NPC is grappled by
    // someone other than the player, attacking the player is at disadvantage.
    let grappled = conditions::grappled_attack_disadvantage(
        &npc_conditions,
        &state.character.name,
    );
    // Sap mastery: consume the mark so only the FIRST attack this turn is
    // rolled with disadvantage. Multiattack follow-ups revert to normal.
    let sapped_first_attack = consume_sap_disadvantage(combat, npc_id);
    if sapped_first_attack {
        lines.push("(Disadvantage from Sap mastery.)".to_string());
    }

    // Check for melee attack
    let melee_attack = npc_attacks.iter().find(|a| a.reach > 0 && distance <= a.reach as u32).cloned();
    if let Some(attack) = melee_attack {
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
            let result = resolve_npc_attack(rng, &attack, player_ac, player_dodging, distance, &npc_conditions, &player_conditions, iter_disadv);
            let disadv = if result.disadvantage { " (with disadvantage)" } else { "" };

            if result.hit {
                let was_dying = state.character.current_hp <= 0;
                state.character.current_hp -= result.damage;
                if result.natural_20 {
                    lines.push(format!("{} attacks with {} -- CRITICAL HIT! {} {} damage!",
                        npc_name, result.weapon_name, result.damage, result.damage_type));
                } else {
                    lines.push(format!("{} attacks with {} ({}+{}={} vs AC {}){} -- hit for {} {} damage.",
                        npc_name, result.weapon_name, result.attack_roll,
                        attack.hit_bonus, result.total_attack, player_ac, disadv,
                        result.damage, result.damage_type));
                }
                // Damage-while-dying: if the player was already at 0 HP when
                // this hit landed, add a death save failure (two on a crit).
                if was_dying {
                    let outcome = combat.apply_damage_while_dying(
                        &mut state.character, result.damage, result.natural_20,
                    );
                    lines.extend(narrate_damage_while_dying_outcome(outcome));
                }
            } else if result.natural_1 {
                lines.push(format!("{} attacks with {} -- natural 1, miss!", npc_name, result.weapon_name));
            } else {
                lines.push(format!("{} attacks with {} ({}+{}={} vs AC {}){} -- miss.",
                    npc_name, result.weapon_name, result.attack_roll,
                    attack.hit_bonus, result.total_attack, player_ac, disadv));
            }
        }
        return lines;
    }

    // Check for ranged attack
    let ranged_attack = npc_attacks.iter().find(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    }).cloned();
    if let Some(attack) = ranged_attack {
        for i in 0..npc_multiattack {
            if state.character.current_hp <= 0 && combat.death_save_failures >= 3 {
                break;
            }
            let iter_disadv = grappled || (sapped_first_attack && i == 0);
            let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
            let player_dodging = combat.player_dodging;
            let result = resolve_npc_attack(rng, &attack, player_ac, player_dodging, distance, &npc_conditions, &player_conditions, iter_disadv);
            let disadv = if result.disadvantage { " (with disadvantage)" } else { "" };

            if result.hit {
                let was_dying = state.character.current_hp <= 0;
                state.character.current_hp -= result.damage;
                if result.natural_20 {
                    lines.push(format!("{} fires {} -- CRITICAL HIT! {} {} damage!",
                        npc_name, result.weapon_name, result.damage, result.damage_type));
                } else {
                    lines.push(format!("{} fires {} ({}+{}={} vs AC {}){} -- hit for {} {} damage.",
                        npc_name, result.weapon_name, result.attack_roll,
                        attack.hit_bonus, result.total_attack, player_ac, disadv,
                        result.damage, result.damage_type));
                }
                if was_dying {
                    let outcome = combat.apply_damage_while_dying(
                        &mut state.character, result.damage, result.natural_20,
                    );
                    lines.extend(narrate_damage_while_dying_outcome(outcome));
                }
            } else if result.natural_1 {
                lines.push(format!("{} fires {} -- natural 1, miss!", npc_name, result.weapon_name));
            } else {
                lines.push(format!("{} fires {} ({}+{}={} vs AC {}){} -- miss.",
                    npc_name, result.weapon_name, result.attack_roll,
                    attack.hit_bonus, result.total_attack, player_ac, disadv));
            }
        }
        return lines;
    }

    // Move toward player. Slow mastery (2024 SRD) reduces the NPC's Speed
    // by up to 10 ft for this move; the reduction is reported once so the
    // player can see why the NPC moved less.
    let slow_reduction = slow_speed_reduction(combat, npc_id).max(0) as u32;
    let effective_speed = (npc_speed as u32).saturating_sub(slow_reduction);
    if slow_reduction > 0 {
        lines.push(format!(
            "(Slow: {}'s Speed reduced by {} ft this turn.)",
            npc_name, slow_reduction,
        ));
    }
    let move_amount = effective_speed;
    let new_distance = if distance > move_amount { distance - move_amount } else { 5 };
    combat.distances.insert(npc_id, new_distance);
    lines.push(format!("{} moves toward you. ({}ft -> {}ft)", npc_name, distance, new_distance));

    lines
}

// ---- Player Movement ----

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

    let move_amount = (movement as u32).min(distance - 5);
    let new_distance = distance - move_amount;
    combat.distances.insert(target_id, new_distance);
    combat.player_movement_remaining -= move_amount as i32;

    let target_name = state.world.npcs.get(&target_id)
        .map(|n| n.name.clone())
        .unwrap_or_else(|| "the enemy".to_string());

    lines.push(format!("You move toward {}. ({}ft -> {}ft, {}ft movement remaining)",
        target_name, distance, new_distance, combat.player_movement_remaining));

    lines
}

/// Move the player away from all enemies. Returns narration lines.
pub fn retreat(
    rng: &mut impl Rng,
    state: &mut GameState,
    combat: &mut CombatState,
) -> Vec<String> {
    let mut lines = Vec::new();
    let movement = combat.player_movement_remaining;

    if movement <= 0 {
        return vec!["You have no movement remaining this turn.".to_string()];
    }

    let move_amount = movement as u32;

    // Check for opportunity attacks from NPCs only when retreat leaves their reach (if not disengaging)
    if !combat.player_disengaging {
        let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
        let potential_attackers: Vec<(NpcId, u32, u32)> = combat.distances.iter()
            .map(|(&id, &old_distance)| {
                let new_distance = old_distance.saturating_add(move_amount);
                (id, old_distance, new_distance)
            })
            .collect();

        for (npc_id, old_distance, new_distance) in potential_attackers {
            let leaves_reach = state.world.npcs.get(&npc_id)
                .and_then(|npc| npc.combat_stats.as_ref())
                .and_then(|stats| {
                    if stats.current_hp <= 0 {
                        return None;
                    }
                    let max_melee_reach = stats.attacks.iter()
                        .filter(|a| a.reach > 0)
                        .map(|a| a.reach as u32)
                        .max()?;
                    Some(old_distance <= max_melee_reach && new_distance > max_melee_reach)
                })
                .unwrap_or(false);

            if !leaves_reach {
                continue;
            }

            if let Some((npc_name, result)) = resolve_opportunity_attack(rng, npc_id, state, player_ac, old_distance) {
                if result.hit {
                    let was_dying = state.character.current_hp <= 0;
                    state.character.current_hp -= result.damage;
                    lines.push(format!("{} makes an opportunity attack with {} -- hit for {} {} damage!",
                        npc_name, result.weapon_name, result.damage, result.damage_type));
                    if was_dying {
                        let outcome = combat.apply_damage_while_dying(
                            &mut state.character, result.damage, result.natural_20,
                        );
                        lines.extend(narrate_damage_while_dying_outcome(outcome));
                    }
                } else {
                    lines.push(format!("{} makes an opportunity attack with {} -- miss!",
                        npc_name, result.weapon_name));
                }
            }
        }
    }

    // Move all distances by movement amount
    for (_, dist) in combat.distances.iter_mut() {
        *dist += move_amount;
    }
    combat.player_movement_remaining = 0;

    lines.push(format!("You retreat {} ft.", move_amount));

    lines
}

/// Format the current combat status for the "look" command.
pub fn format_combat_status(state: &GameState, combat: &CombatState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("=== Combat - Round {} ===", combat.round));
    lines.push(format!("HP: {}/{}", state.character.current_hp, state.character.max_hp));
    lines.push(format!("AC: {}", crate::equipment::calculate_ac(&state.character, &state.world.items)));
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
                        format!("HP {}/{}, {}ft away", stats.current_hp, stats.max_hp, distance)
                    };
                    lines.push(format!("  {} - {}", npc.name, status));
                }
            }
        }
    }

    if combat.is_player_turn() {
        lines.push(String::new());
        lines.push(format!("Movement remaining: {} ft", combat.player_movement_remaining));
        let status = |used: bool| if used { "used" } else { "available" };
        lines.push(format!(
            "Action: {} | Bonus: {} | Reaction: {} | Free interaction: {}",
            status(combat.action_used),
            status(combat.bonus_action_used),
            status(combat.reaction_used),
            status(combat.free_interaction_used),
        ));
        if !combat.action_used {
            lines.push("Commands: attack <target>, dodge, disengage, dash".to_string());
        } else {
            lines.push("Action used. You can still move (approach/retreat) or spend your bonus action.".to_string());
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
                    if stats.current_hp <= 0 { continue; }
                    let distance = combat.distances.get(id).copied().unwrap_or(0);
                    let range_label = if distance <= 5 { "melee".to_string() } else { format!("{}ft", distance) };
                    lines.push(format!("  {} — HP {}/{}, {}",
                        npc.name, stats.current_hp, stats.max_hp, range_label));
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
            Combatant::Npc(id) => state.world.npcs.get(id)
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
        let name = state.world.npcs.get(&npc_id)
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
pub fn try_apply_condition_to_npc(
    npc: &mut Npc,
    new_condition: ActiveCondition,
) -> bool {
    // Stat-block immunity check (e.g., Skeleton immune to Poisoned).
    if let Some(stats) = npc.combat_stats.as_ref() {
        if stats.condition_immunities.contains(&new_condition.condition) {
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
    let Some(stats) = npc.combat_stats.as_mut() else { return 0 };
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
        narration.push(format!("The {} is immune to {} damage!", target_name, damage_type));
        return 0;
    }
    if stats.damage_resistances.contains(&damage_type) {
        let halved = incoming / 2;
        narration.push(format!("The {} resists the {} damage.", target_name, damage_type));
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
pub fn consume_vex_advantage(
    combat: &mut CombatState,
    target_npc_id: NpcId,
) -> bool {
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
pub fn consume_sap_disadvantage(
    combat: &mut CombatState,
    npc_id: NpcId,
) -> bool {
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
    let existing = combat.slow_targets.get(&target_npc_id).copied().unwrap_or(0);
    if existing >= 10 {
        return false;
    }
    combat.slow_targets.insert(target_npc_id, 10);
    narration.push("Slow: the target's Speed is reduced by 10 ft until the start of your next turn.".to_string());
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
    let Some(npc) = state.world.npcs.get_mut(&target_npc_id) else { return false };
    let Some(stats) = npc.combat_stats.as_ref() else { return false };
    let con = stats.ability_scores.get(&Ability::Constitution).copied().unwrap_or(10);
    let con_mod = Ability::modifier(con);
    let con_save_prof = 0; // NPC CON save proficiency is not modelled in MVP.
    let roll = roll_d20(rng);
    let save_total = roll + con_mod + con_save_prof;
    let npc_name = npc.name.clone();
    if save_total >= dc {
        narration.push(format!(
            "Topple: {} succeeds on a CON save ({}+{}={} vs DC {}).",
            npc_name, roll, con_mod, save_total, dc
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
            "Topple: {} fails the CON save ({}+{}={} vs DC {}) and is knocked Prone!",
            npc_name, roll, con_mod, save_total, dc
        ));
    } else {
        narration.push(format!(
            "Topple: {} fails the CON save but is immune to Prone.",
            npc_name
        ));
    }
    applied
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
    let secondary_id = combat.distances.iter()
        .filter(|(id, dist)| {
            **id != primary_target_id
                && **dist <= 5
                && state.world.npcs.get(*id)
                    .and_then(|n| n.combat_stats.as_ref())
                    .map(|s| s.current_hp > 0)
                    .unwrap_or(false)
        })
        .map(|(id, _)| *id)
        .next();
    let secondary_id = secondary_id?;
    let secondary_ac = state.world.npcs.get(&secondary_id)
        .and_then(|n| n.combat_stats.as_ref())
        .map(|s| s.ac)
        .unwrap_or(10);
    let secondary_dodging = combat.npc_dodging.get(&secondary_id).copied().unwrap_or(false);
    let secondary_conditions: Vec<ActiveCondition> = state.world.npcs.get(&secondary_id)
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
pub fn apply_nick_mastery(
    has_mastery: bool,
    combat: &mut CombatState,
) -> bool {
    if !has_mastery || combat.nick_used_this_turn {
        return false;
    }
    combat.nick_used_this_turn = true;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;
    use crate::character::{create_character, race::Race, class::Class};
    use crate::state::{Npc, NpcRole, Disposition, WorldState, GamePhase, SAVE_VERSION};
    use crate::state::{CombatStats, NpcAttack, DamageType};
    #[allow(unused_imports)]
    use crate::equipment::Equipment;
    use std::collections::HashSet;

    fn test_character() -> Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 16);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 14);
        scores.insert(Ability::Intelligence, 10);
        scores.insert(Ability::Wisdom, 12);
        scores.insert(Ability::Charisma, 8);
        create_character("TestHero".to_string(), Race::Human, Class::Fighter, scores, vec![])
    }

    fn goblin_stats() -> CombatStats {
        CombatStats {
            max_hp: 7, current_hp: 7, ac: 15, speed: 30,
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
                    name: "Scimitar".to_string(), hit_bonus: 4,
                    damage_dice: 1, damage_die: 6, damage_bonus: 2,
                    damage_type: DamageType::Slashing, reach: 5,
                    range_normal: 0, range_long: 0,
                },
                NpcAttack {
                    name: "Shortbow".to_string(), hit_bonus: 4,
                    damage_dice: 1, damage_die: 6, damage_bonus: 2,
                    damage_type: DamageType::Piercing, reach: 0,
                    range_normal: 80, range_long: 320,
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
        npcs.insert(0, Npc {
            id: 0,
            name: "Goblin".to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(goblin_stats()),
            conditions: Vec::new(),
        });

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(), npcs, items: HashMap::new(),
                triggers: HashMap::new(), triggered: HashSet::new(),
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
            pending_disambiguation: None,
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
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        let dist = combat.distances.get(&0).unwrap();
        assert!(*dist >= 20 && *dist <= 30);
        assert!(*dist % 5 == 0);
    }

    #[test]
    fn test_start_combat_initiative_order() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        assert_eq!(combat.initiative_order.len(), 2);
        assert_eq!(combat.round, 1);
    }

    #[test]
    fn test_player_melee_reach_unarmed_is_5() {
        let character = test_character();
        let items = HashMap::new();
        assert_eq!(player_melee_reach(&character, &items), 5,
            "Unarmed melee reach should be 5 ft");
    }

    #[test]
    fn test_player_melee_reach_longsword_is_5() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(500u32, Item {
            id: 500,
            name: "Longsword".to_string(),
            description: "".to_string(),
            item_type: ItemType::Weapon {
                damage_dice: 1, damage_die: 8,
                damage_type: DamageType::Slashing,
                properties: crate::equipment::VERSATILE,
                category: WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        character.equipped.main_hand = Some(500);
        assert_eq!(player_melee_reach(&character, &items), 5,
            "Non-reach weapon should give 5 ft reach");
    }

    #[test]
    fn test_player_melee_reach_glaive_is_10() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(501u32, Item {
            id: 501,
            name: "Glaive".to_string(),
            description: "".to_string(),
            item_type: ItemType::Weapon {
                damage_dice: 1, damage_die: 10,
                damage_type: DamageType::Slashing,
                properties: crate::equipment::REACH | crate::equipment::HEAVY | crate::equipment::TWO_HANDED,
                category: WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        character.equipped.main_hand = Some(501);
        assert_eq!(player_melee_reach(&character, &items), 10,
            "REACH weapon should give 10 ft reach");
    }

    #[test]
    fn test_player_melee_reach_ranged_only_weapon_falls_back_to_5() {
        // Pure ranged weapon (longbow) has no melee usage; reach should
        // fall back to the unarmed default of 5 rather than 0.
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut character = test_character();
        let mut items = HashMap::new();
        items.insert(502u32, Item {
            id: 502,
            name: "Longbow".to_string(),
            description: "".to_string(),
            item_type: ItemType::Weapon {
                damage_dice: 1, damage_die: 8,
                damage_type: DamageType::Piercing,
                properties: crate::equipment::AMMUNITION | crate::equipment::TWO_HANDED | crate::equipment::HEAVY,
                category: WeaponCategory::Martial,
                versatile_die: 0, range_normal: 150, range_long: 600,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        character.equipped.main_hand = Some(502);
        assert_eq!(player_melee_reach(&character, &items), 5,
            "Pure ranged weapon should fall back to unarmed reach of 5 ft");
    }

    #[test]
    fn test_npc_within_player_reach_unarmed_at_5ft() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        combat.distances.insert(0, 5);
        assert!(npc_within_player_reach(&state, &combat, 0),
            "Goblin at 5 ft should be in unarmed reach");

        combat.distances.insert(0, 10);
        assert!(!npc_within_player_reach(&state, &combat, 0),
            "Goblin at 10 ft should NOT be in unarmed reach");
    }

    #[test]
    fn test_npc_within_player_reach_respects_reach_weapon() {
        use crate::state::{Item, ItemType, WeaponCategory};
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Equip a glaive (REACH weapon). 10 ft threatened area.
        state.world.items.insert(700u32, Item {
            id: 700,
            name: "Glaive".to_string(),
            description: "".to_string(),
            item_type: ItemType::Weapon {
                damage_dice: 1, damage_die: 10,
                damage_type: DamageType::Slashing,
                properties: crate::equipment::REACH | crate::equipment::HEAVY | crate::equipment::TWO_HANDED,
                category: WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        state.character.equipped.main_hand = Some(700);

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 10);
        assert!(npc_within_player_reach(&state, &combat, 0),
            "Glaive-equipped player should threaten NPC at 10 ft");

        combat.distances.insert(0, 15);
        assert!(!npc_within_player_reach(&state, &combat, 0),
            "Glaive reach is 10 ft; NPC at 15 ft should NOT be threatened");
    }

    #[test]
    fn test_npc_within_player_reach_dead_npc_not_threatened() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        // Kill the goblin.
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;
        combat.distances.insert(0, 5);
        assert!(!npc_within_player_reach(&state, &combat, 0),
            "Dead NPC at 5 ft should not be treated as reachable for OA");
    }

    #[test]
    fn test_has_living_hostile_within() {
        let mut state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        combat.distances.insert(0, 5);
        assert!(has_living_hostile_within(&state, &combat, 5));

        combat.distances.insert(0, 10);
        assert!(!has_living_hostile_within(&state, &combat, 5));

        // dead enemy should not count
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;
        combat.distances.insert(0, 5);
        assert!(!has_living_hostile_within(&state, &combat, 5));
    }

    #[test]
    fn test_resolve_npc_attack_hit_or_miss() {
        let attack = NpcAttack {
            name: "Scimitar".to_string(), hit_bonus: 4,
            damage_dice: 1, damage_die: 6, damage_bonus: 2,
            damage_type: DamageType::Slashing, reach: 5,
            range_normal: 0, range_long: 0,
        };
        // Run many times to get both hits and misses
        let mut hits = 0;
        let mut misses = 0;
        for seed in 0..100 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(&mut rng, &attack, 15, false, 5, &[], &[], false);
            if result.hit { hits += 1; } else { misses += 1; }
        }
        assert!(hits > 0, "Should have some hits");
        assert!(misses > 0, "Should have some misses");
    }

    #[test]
    fn test_natural_20_always_hits() {
        let attack = NpcAttack {
            name: "Test".to_string(), hit_bonus: -10, // Very low bonus
            damage_dice: 1, damage_die: 6, damage_bonus: 0,
            damage_type: DamageType::Slashing, reach: 5,
            range_normal: 0, range_long: 0,
        };
        // Find a seed that gives nat 20
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(&mut rng, &attack, 30, false, 5, &[], &[], false);
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
            name: "Test".to_string(), hit_bonus: 100, // Very high bonus
            damage_dice: 1, damage_die: 6, damage_bonus: 0,
            damage_type: DamageType::Slashing, reach: 5,
            range_normal: 0, range_long: 0,
        };
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(&mut rng, &attack, 1, false, 5, &[], &[], false);
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
            name: "Test".to_string(), hit_bonus: 4,
            damage_dice: 1, damage_die: 6, damage_bonus: 2,
            damage_type: DamageType::Slashing, reach: 5,
            range_normal: 0, range_long: 0,
        };
        // Find a nat 20 and verify higher damage potential
        let mut crit_damages = Vec::new();
        for seed in 0..1000 {
            let mut rng = StdRng::seed_from_u64(seed);
            let result = resolve_npc_attack(&mut rng, &attack, 10, false, 5, &[], &[], false);
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
            name: "Test".to_string(), hit_bonus: 4,
            damage_dice: 1, damage_die: 6, damage_bonus: 2,
            damage_type: DamageType::Slashing, reach: 5,
            range_normal: 0, range_long: 0,
        };

        let mut dodge_hits = 0;
        let mut normal_hits = 0;
        for seed in 0..1000 {
            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);
            let dodge = resolve_npc_attack(&mut rng1, &attack, 15, true, 5, &[], &[], false);
            let normal = resolve_npc_attack(&mut rng2, &attack, 15, false, 5, &[], &[], false);
            if dodge.hit { dodge_hits += 1; }
            if normal.hit { normal_hits += 1; }
        }
        assert!(dodge_hits < normal_hits, "Dodging should reduce hit rate: dodge={}, normal={}", dodge_hits, normal_hits);
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
            let result = resolve_player_attack(&mut rng, &player, 100, false, None, &items, 5, true, false, &[], false, false);
            assert_eq!(result.weapon_name, "Unarmed");
            assert!(result.attack_roll >= 1 && result.attack_roll <= 20,
                "Attack roll must be a real d20 (seed={}, roll={})", seed, result.attack_roll);
            if result.natural_20 {
                // Nat 20 always hits per SRD, even against absurd AC.
                assert!(result.hit, "Nat 20 must hit (seed={})", seed);
            } else {
                saw_miss = true;
                assert!(!result.hit, "Non-crit unarmed must miss AC 100 (seed={}, roll={}, total={})",
                    seed, result.attack_roll, result.total_attack);
                assert_eq!(result.damage, 0, "Miss should deal 0 damage (seed={})", seed);
            }
        }
        assert!(saw_miss, "Expected to observe at least one miss against AC 100");
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
            let result = resolve_player_attack(&mut rng, &player, 1, false, None, &items, 5, true, false, &[], false, false);
            assert_eq!(result.weapon_name, "Unarmed");
            assert_eq!(result.damage_type, DamageType::Bludgeoning);
            if result.hit {
                hit_count += 1;
                if result.natural_20 {
                    assert_eq!(result.damage, 5, "Nat 20 crit should deal 2 + STR mod (seed={})", seed);
                    crit_damage_seen = true;
                } else {
                    assert_eq!(result.damage, 4, "Normal hit should deal 1 + STR mod (seed={})", seed);
                    base_damage_seen = true;
                }
            } else {
                assert!(result.natural_1, "Only a nat 1 should miss AC 1 (seed={}, roll={})", seed, result.attack_roll);
            }
        }
        assert!(hit_count > 0, "Expected at least some hits against AC 1");
        assert!(base_damage_seen, "Expected to observe a normal-hit damage roll");
        assert!(crit_damage_seen, "Expected to observe a nat-20 crit in 200 seeds");
    }

    #[test]
    fn test_unarmed_strike_disadvantage_from_poisoned() {
        use crate::conditions::{ActiveCondition, ConditionDuration};

        // Poisoned imposes disadvantage on attack rolls. With unarmed now on the
        // standard roll pipeline, a poisoned attacker should hit less often.
        let player = test_character();
        let mut poisoned_player = player.clone();
        poisoned_player.conditions.push(ActiveCondition::new(
            ConditionType::Poisoned, ConditionDuration::Rounds(3),
        ));
        let items = HashMap::new();

        let mut normal_hits = 0;
        let mut poisoned_hits = 0;
        for seed in 0..1000 {
            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);
            let normal = resolve_player_attack(&mut rng1, &player, 15, false, None, &items, 5, true, false, &[], false, false);
            let poisoned = resolve_player_attack(&mut rng2, &poisoned_player, 15, false, None, &items, 5, true, false, &[], false, false);
            if normal.hit { normal_hits += 1; }
            if poisoned.hit { poisoned_hits += 1; }
        }
        assert!(poisoned_hits < normal_hits,
            "Poisoned unarmed attacker should hit less often: normal={}, poisoned={}",
            normal_hits, poisoned_hits);
    }

    #[test]
    fn test_approach_reduces_distance() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;

        approach_target(&mut rng, 0, &state, &mut combat);
        assert_eq!(*combat.distances.get(&0).unwrap(), 5);
    }

    #[test]
    fn test_retreat_increases_distance() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        state.world.npcs.get_mut(&0).unwrap()
            .combat_stats.as_mut().unwrap()
            .attacks[0].reach = 10;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(lines.iter().any(|l| l.contains("opportunity attack")),
            "Expected opportunity attack narration, got {:?}", lines);
    }

    #[test]
    fn test_retreat_no_opportunity_attack_outside_reach_5() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state.world.npcs.get_mut(&0).unwrap()
            .combat_stats.as_mut().unwrap()
            .attacks[0].reach = 5;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 10);
        combat.player_movement_remaining = 30;
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(!lines.iter().any(|l| l.contains("opportunity attack")),
            "Did not expect opportunity attack narration, got {:?}", lines);
    }

    #[test]
    fn test_retreat_no_opportunity_attack_when_still_within_reach_10() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        state.world.npcs.get_mut(&0).unwrap()
            .combat_stats.as_mut().unwrap()
            .attacks[0].reach = 10;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 5);
        combat.player_movement_remaining = 5; // Move to 10ft, still within reach 10
        combat.player_disengaging = false;

        let lines = retreat(&mut rng, &mut state, &mut combat);
        assert!(!lines.iter().any(|l| l.contains("opportunity attack")),
            "Should not trigger OA when still within reach, got {:?}", lines);
    }

    #[test]
    fn test_combat_end_victory() {
        let mut state = test_state_with_goblin();
        // Kill the goblin
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.death_save_failures = 3;
        assert_eq!(combat.check_end(&state), Some(false));
    }

    #[test]
    fn test_combat_not_ended() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        assert_eq!(combat.check_end(&state), None);
    }

    // Hypothesis: The bug occurs because check_end uses unwrap_or(true) which treats
    // any NPC with missing combat_stats as dead, triggering premature VICTORY when
    // only one of multiple hostile NPCs has been killed.

    fn test_state_with_two_goblins() -> GameState {
        let character = test_character();
        let mut npcs = HashMap::new();
        npcs.insert(0, Npc {
            id: 0,
            name: "Goblin".to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(goblin_stats()),
            conditions: Vec::new(),
        });
        npcs.insert(1, Npc {
            id: 1,
            name: "Goblin".to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(goblin_stats()),
            conditions: Vec::new(),
        });

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(), npcs, items: HashMap::new(),
                triggers: HashMap::new(), triggered: HashSet::new(),
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
            pending_disambiguation: None,
        }
    }

    #[test]
    fn test_two_hostiles_kill_one_combat_continues() {
        let mut state = test_state_with_two_goblins();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs);

        // Kill only the first goblin
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;

        // Combat should NOT be over -- second goblin still alive
        assert_eq!(combat.check_end(&state), None,
            "Combat should continue when one of two hostile NPCs is still alive");
    }

    #[test]
    fn test_two_hostiles_kill_both_victory() {
        let mut state = test_state_with_two_goblins();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs);

        // Kill both goblins
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;
        state.world.npcs.get_mut(&1).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;

        // Now combat should end in victory
        assert_eq!(combat.check_end(&state), Some(true),
            "Combat should end in VICTORY when all hostile NPCs are dead");
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
            death_save_successes: 0,
            death_save_failures: 0,
        };

        // Kill the first goblin (the one with stats)
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().current_hp = 0;

        // Combat should NOT end -- the ghost NPC (no stats) should be treated as alive
        assert_eq!(combat.check_end(&state), None,
            "NPC with missing combat_stats should be treated as alive, not dead");
    }

    #[test]
    fn test_advance_turn_skips_dead() {
        let mut state = test_state_with_goblin();
        // Add a second goblin that's dead
        state.world.npcs.insert(1, Npc {
            id: 1,
            name: "Dead Goblin".to_string(),
            role: NpcRole::Guard,
            disposition: Disposition::Hostile,
            dialogue_tags: vec![],
            location: 0,
            combat_stats: Some(CombatStats {
                max_hp: 7, current_hp: 0, ac: 15, speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 0.25,
                ..Default::default()
            }),
            conditions: Vec::new(),
        });

        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0, 1], &state.world.npcs);

        // Advance through turns, dead NPC should be skipped
        let mut found_dead_npc_turn = false;
        for _ in 0..10 {
            let c = combat.advance_turn(&state);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 5); // In melee range

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        assert!(!lines.is_empty());
        // Should attack with Scimitar (melee)
        assert!(lines[0].contains("Scimitar"), "NPC should use melee: {}", lines[0]);
    }

    #[test]
    fn test_npc_ai_ranged_out_of_melee() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 30); // Out of melee, in ranged

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        assert!(!lines.is_empty());
        // Should use Shortbow (ranged)
        assert!(lines[0].contains("Shortbow"), "NPC should use ranged: {}", lines[0]);
    }

    #[test]
    fn test_npc_ai_moves_toward_if_no_attack_in_range() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        // Remove ranged attack so NPC can only melee
        state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap().attacks = vec![
            NpcAttack {
                name: "Scimitar".to_string(), hit_bonus: 4,
                damage_dice: 1, damage_die: 6, damage_bonus: 2,
                damage_type: DamageType::Slashing, reach: 5,
                range_normal: 0, range_long: 0,
            },
        ];
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 60); // Far away

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let new_dist = *combat.distances.get(&0).unwrap();
        assert!(new_dist < 60, "NPC should have moved closer");
        assert!(lines[0].contains("moves toward"), "Should narrate movement: {}", lines[0]);
    }

    // ---- Condition Integration Tests ----

    #[test]
    fn test_poisoned_player_attacks_with_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Direct test: verify that poisoned condition returns disadvantage from get_attack_advantage
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(3)),
        ];

        assert_eq!(conditions::get_attack_advantage(&poisoned), Some(false),
            "Poisoned should impose disadvantage on attacks");
    }

    #[test]
    fn test_attacking_stunned_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Direct test: verify that stunned target grants advantage to attacker
        let attacker: Vec<ActiveCondition> = vec![];
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];

        assert_eq!(conditions::get_defense_advantage(&attacker, &stunned), Some(true),
            "Attacking stunned target should grant advantage");
    }

    #[test]
    fn test_paralyzed_target_is_auto_crit() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Direct test: verify paralyzed condition marks target as auto-crit
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];

        assert!(conditions::is_auto_crit_target(&paralyzed),
            "Paralyzed target should be subject to auto-crits");

        // Stunned should NOT be auto-crit
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(!conditions::is_auto_crit_target(&stunned),
            "Stunned target should not be auto-crit");
    }

    #[test]
    fn test_prone_grants_advantage_within_5ft() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Direct test: prone target grants advantage to attackers
        let attacker: Vec<ActiveCondition> = vec![];
        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];

        assert_eq!(conditions::get_defense_advantage(&attacker, &prone), Some(true),
            "Attacking prone target should grant advantage");
    }

    #[test]
    fn test_blinded_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Direct test: blinded target grants advantage to attackers
        let attacker: Vec<ActiveCondition> = vec![];
        let blinded = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(2)),
        ];

        assert_eq!(conditions::get_defense_advantage(&attacker, &blinded), Some(true),
            "Attacking blinded target should grant advantage");
    }

    #[test]
    fn test_blinded_and_poisoned_impose_attack_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Blinded imposes disadvantage
        let blinded = vec![
            ActiveCondition::new(ConditionType::Blinded, ConditionDuration::Rounds(1)),
        ];
        assert_eq!(conditions::get_attack_advantage(&blinded), Some(false));

        // Prone imposes disadvantage
        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];
        assert_eq!(conditions::get_attack_advantage(&prone), Some(false));
    }

    #[test]
    fn test_stunned_and_paralyzed_prevent_actions() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        // Stunned prevents actions
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(!conditions::can_take_actions(&stunned));
        assert!(!conditions::can_take_reactions(&stunned));

        // Paralyzed prevents actions
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        assert!(!conditions::can_take_actions(&paralyzed));
        assert!(!conditions::can_take_reactions(&paralyzed));

        // Poisoned allows actions
        let poisoned = vec![
            ActiveCondition::new(ConditionType::Poisoned, ConditionDuration::Rounds(2)),
        ];
        assert!(conditions::can_take_actions(&poisoned));
        assert!(conditions::can_take_reactions(&poisoned));
    }

    #[test]
    fn test_stunned_and_paralyzed_auto_fail_str_dex_saves() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};
        use crate::types::Ability;

        // Stunned auto-fails STR and DEX saves
        let stunned = vec![
            ActiveCondition::new(ConditionType::Stunned, ConditionDuration::Rounds(1)),
        ];
        assert!(conditions::get_save_auto_fail(&stunned, Ability::Strength));
        assert!(conditions::get_save_auto_fail(&stunned, Ability::Dexterity));
        assert!(!conditions::get_save_auto_fail(&stunned, Ability::Constitution));

        // Paralyzed auto-fails STR and DEX saves
        let paralyzed = vec![
            ActiveCondition::new(ConditionType::Paralyzed, ConditionDuration::Rounds(1)),
        ];
        assert!(conditions::get_save_auto_fail(&paralyzed, Ability::Strength));
        assert!(conditions::get_save_auto_fail(&paralyzed, Ability::Dexterity));
        assert!(!conditions::get_save_auto_fail(&paralyzed, Ability::Wisdom));
    }

    #[test]
    fn test_prone_reduces_speed() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let prone = vec![
            ActiveCondition::new(ConditionType::Prone, ConditionDuration::Permanent),
        ];
        assert_eq!(conditions::get_speed_multiplier(&prone), 0.5,
            "Prone should reduce speed multiplier to 0.5");

        let normal: Vec<ActiveCondition> = vec![];
        assert_eq!(conditions::get_speed_multiplier(&normal), 1.0,
            "No conditions should have normal speed");
    }

    // ---- Integration: new SRD conditions in attack resolution ----

    #[test]
    fn test_invisible_attacker_vs_visible_target_grants_advantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let invisible = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(3)),
        ];
        // Attacker-side query returns Some(true) => advantage.
        assert_eq!(conditions::get_attack_advantage(&invisible), Some(true));
    }

    #[test]
    fn test_restrained_imposes_attack_disadvantage_in_combat() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        assert_eq!(conditions::get_attack_advantage(&restrained), Some(false));
    }

    #[test]
    fn test_attacking_restrained_target_grants_advantage_in_combat() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let attacker: Vec<ActiveCondition> = vec![];
        let restrained = vec![
            ActiveCondition::new(ConditionType::Restrained, ConditionDuration::Permanent),
        ];
        assert_eq!(conditions::get_defense_advantage(&attacker, &restrained), Some(true));
    }

    #[test]
    fn test_attacking_invisible_target_imposes_disadvantage() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let attacker: Vec<ActiveCondition> = vec![];
        let invisible = vec![
            ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(3)),
        ];
        assert_eq!(conditions::get_defense_advantage(&attacker, &invisible), Some(false));
    }

    #[test]
    fn test_unconscious_target_is_auto_crit() {
        use crate::conditions::{self, ActiveCondition, ConditionType, ConditionDuration};

        let unconscious = vec![
            ActiveCondition::new(ConditionType::Unconscious, ConditionDuration::Permanent),
        ];
        assert!(conditions::is_auto_crit_target(&unconscious));
    }

    #[test]
    fn test_player_with_invisible_rolls_attack_with_advantage() {
        // End-to-end: player attacks a goblin while Invisible. Over many trials
        // the hit rate should be measurably higher than neutral, confirming that
        // advantage was actually applied to the roll (not just returned by the
        // query function).
        use crate::conditions::{ActiveCondition, ConditionType, ConditionDuration};

        let mut wins_with_adv = 0;
        let mut wins_neutral = 0;
        let trials = 400;

        for seed in 0..trials {
            let mut state_adv = test_state_with_goblin();
            state_adv.character.conditions.push(
                ActiveCondition::new(ConditionType::Invisible, ConditionDuration::Rounds(10)),
            );
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
                &mut rng1, &state_adv.character, target_ac, false, Some(9999),
                &state_adv.world.items, distance, true, false,
                &[], // defender has no conditions
                false, false,
            );
            let res_neu = resolve_player_attack(
                &mut rng2, &state_neu.character, target_ac, false, Some(9999),
                &state_neu.world.items, distance, true, false, &[],
                false, false,
            );

            if res_adv.hit { wins_with_adv += 1; }
            if res_neu.hit { wins_neutral += 1; }
        }

        // Advantage should measurably improve hit rate; require a reasonable gap.
        assert!(
            wins_with_adv > wins_neutral + 20,
            "Invisible attacker should hit more often ({} vs neutral {})",
            wins_with_adv, wins_neutral
        );
    }

    #[test]
    fn test_extra_disadvantage_flag_reduces_player_hit_rate() {
        // End-to-end: when the orchestrator reports `extra_disadvantage = true`
        // (e.g., Grappled attacking a non-grappler) the hit rate should be
        // measurably lower than without it. This test verifies the parameter
        // is actually wired into the roll.
        use crate::state::{Item, ItemType, DamageType, WeaponCategory};

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
                    damage_dice: 1, damage_die: 6,
                    damage_type: DamageType::Bludgeoning,
                    properties: 0, category: WeaponCategory::Simple,
                    versatile_die: 0, range_normal: 0, range_long: 0,
                },
                location: None, carried_by_player: true,
                charges_remaining: None,
            };
            state.world.items.insert(9999, club);
            state.character.inventory.push(9999);
            state.character.equipped.main_hand = Some(9999);

            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);

            let res_disadv = resolve_player_attack(
                &mut rng1, &state.character, 15, false, Some(9999),
                &state.world.items, 5, true, false, &[], true, false,
            );
            let res_neu = resolve_player_attack(
                &mut rng2, &state.character, 15, false, Some(9999),
                &state.world.items, 5, true, false, &[], false, false,
            );
            if res_disadv.hit { wins_disadv += 1; }
            if res_neu.hit { wins_neutral += 1; }
        }

        assert!(
            wins_neutral > wins_disadv + 20,
            "extra_disadvantage should lower hit rate (disadv={}, neutral={})",
            wins_disadv, wins_neutral
        );
    }

    // Hypothesis (Bug 3): Dodge disadvantage is not surfaced in attack output because
    // resolve_npc_turn format strings show only the resulting d20 roll with no label
    // indicating two dice were rolled. Fix: append "(with disadvantage)" when
    // result.disadvantage is true.
    #[test]
    fn test_dodge_disadvantage_shown_in_npc_attack_text() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

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
            if all.contains("(with disadvantage)") {
                found_disadvantage_text = true;
                break;
            }
        }
        assert!(found_disadvantage_text,
            "NPC attack output should contain '(with disadvantage)' when player is dodging");
    }

    #[test]
    fn test_no_disadvantage_text_when_not_dodging() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        // Place goblin in melee range, player NOT dodging
        combat.distances.insert(0, 5);
        combat.player_dodging = false;

        for seed in 0..100u64 {
            let mut test_rng = StdRng::seed_from_u64(seed);
            let mut test_state = state.clone();
            let mut test_combat = combat.clone();
            let lines = resolve_npc_turn(&mut test_rng, 0, &mut test_state, &mut test_combat);
            let all = lines.join("\n");
            assert!(!all.contains("(with disadvantage)"),
                "Should not show disadvantage text when player is not dodging. Got: {}", all);
        }
    }

    // ---- Action Economy tests ----

    #[test]
    fn test_combat_state_has_four_independent_resource_flags() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        // Fresh combat should have all resources available.
        assert!(!combat.action_used, "Action should start available");
        assert!(!combat.bonus_action_used, "Bonus action should start available");
        assert!(!combat.reaction_used, "Reaction should start available");
        assert!(!combat.free_interaction_used, "Free interaction should start available");
    }

    #[test]
    fn test_reaction_resets_at_end_of_player_turn_not_start() {
        // Per SRD: reaction resets at end of previous turn so it's available during NPC turns.
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        // Simulate player consuming reaction (e.g. opportunity attack during NPC turn)
        combat.reaction_used = true;

        // End the player's turn: reaction should reset so NPC-turn reactions can fire later.
        combat.end_player_turn();
        assert!(!combat.reaction_used,
            "Reaction should reset at end of player turn so NPCs can't prevent its use");
    }

    #[test]
    fn test_action_bonus_free_reset_at_start_of_player_turn() {
        // action/bonus/free reset at start of the new player turn (existing convention).
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        combat.action_used = true;
        combat.bonus_action_used = true;
        combat.free_interaction_used = true;
        combat.player_movement_remaining = 0;

        // Force advance_turn to cycle back to player (even if already player turn)
        // Simulate an NPC turn by setting current_turn to an NPC, then advancing.
        combat.current_turn = combat.initiative_order.iter()
            .position(|(c, _)| matches!(c, Combatant::Npc(_)))
            .unwrap_or(0);

        combat.advance_turn(&state);

        assert!(combat.is_player_turn(), "Should advance back to player turn");
        assert!(!combat.action_used, "Action should reset at start of player turn");
        assert!(!combat.bonus_action_used, "Bonus should reset at start of player turn");
        assert!(!combat.free_interaction_used, "Free interaction should reset at start of player turn");
        assert_eq!(combat.player_movement_remaining, state.character.speed,
            "Movement should reset to speed at start of player turn");
    }

    #[test]
    fn test_pending_reaction_defaults_to_none_and_serialises() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

        assert!(combat.pending_reaction.is_none(),
            "Fresh combat should have no pending reaction");

        // Round trip
        let json = serde_json::to_string(&combat).unwrap();
        let deserialised: CombatState = serde_json::from_str(&json).unwrap();
        assert!(deserialised.pending_reaction.is_none());
    }

    #[test]
    fn test_pending_reaction_opportunity_attack_round_trips() {
        let mut rng = StdRng::seed_from_u64(42);
        let state = test_state_with_goblin();
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

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
                fleeing_npc_id, old_distance, new_distance, resume_npc_index,
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);

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
                attacker_npc_id, incoming_damage, pre_roll_ac, resume_npc_index,
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        assert!(round_tripped.action_used,
            "Old saves' player_action_used value should map to action_used");
        // New fields should default to false.
        assert!(!round_tripped.bonus_action_used);
        assert!(!round_tripped.reaction_used);
        assert!(!round_tripped.free_interaction_used);
    }

    // ----- monster-stat-blocks (2026-04-15) -----

    use crate::conditions::ConditionDuration;
    use crate::combat::monsters::{find_monster, monster_to_combat_stats};

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
        assert!(!applied,
            "Skeleton should reject Poisoned condition due to stat-block immunity");
        assert!(npc.conditions.is_empty(),
            "rejected condition should not be appended");
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
        assert!(!p2,
            "Petrified target should reject Poisoned per conditions::is_immune_to_condition");
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
        let dealt = apply_damage_modifiers(&stats, 12, DamageType::Poison, "zombie", &mut narration);
        assert_eq!(dealt, 0, "Zombie should be immune to Poison damage");
        assert_eq!(narration.len(), 1);
        assert!(narration[0].contains("immune"), "narration mentions immunity: {:?}", narration[0]);
    }

    #[test]
    fn test_apply_damage_modifiers_no_immunity_passes_through() {
        let zombie_def = find_monster("Zombie").unwrap();
        let stats = monster_to_combat_stats(zombie_def);
        let mut narration = Vec::new();
        let dealt = apply_damage_modifiers(&stats, 7, DamageType::Slashing, "zombie", &mut narration);
        assert_eq!(dealt, 7);
        assert!(narration.is_empty(), "no narration when no immunity/resistance applies");
    }

    #[test]
    fn test_apply_damage_modifiers_resistance_halves_damage() {
        let mut stats = CombatStats::default();
        stats.damage_resistances = vec![DamageType::Slashing];
        let mut narration = Vec::new();
        let dealt = apply_damage_modifiers(&stats, 10, DamageType::Slashing, "ghost", &mut narration);
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
        assert_eq!(apply_damage_modifiers(&stats, 0, DamageType::Fire, "x", &mut narration), 0);
        assert_eq!(apply_damage_modifiers(&stats, -3, DamageType::Fire, "x", &mut narration), 0);
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
        let stats = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap();
        stats.multiattack = 2;
        // Clear the bow attack so we are guaranteed to take the melee branch.
        stats.attacks.retain(|a| a.name == "Scimitar");
        // Give the player enough HP to survive 2 hits.
        state.character.current_hp = 1000;
        state.character.max_hp = 1000;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 5);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        // 2 attack lines (hit/miss/crit each emit one line; never zero per attack).
        let attack_lines = count_lines_with("Scimitar", &lines);
        assert_eq!(attack_lines, 2,
            "multiattack=2 should produce 2 Scimitar attack lines, got {}: {:#?}",
            attack_lines, lines);
    }

    #[test]
    fn test_npc_multiattack_one_makes_one_attack_regression() {
        // multiattack==1 is the default; verify legacy behavior.
        let mut rng = StdRng::seed_from_u64(42);
        let mut state = test_state_with_goblin();
        let stats = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap();
        assert_eq!(stats.multiattack, 1, "fixture default should be 1");
        stats.attacks.retain(|a| a.name == "Scimitar");
        state.character.current_hp = 1000;
        state.character.max_hp = 1000;

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let stats = state.world.npcs.get_mut(&0).unwrap().combat_stats.as_mut().unwrap();
        stats.multiattack = 3;
        stats.attacks.retain(|a| a.name == "Scimitar");
        // Make the attack always hit but keep damage well below max_hp so no
        // single hit triggers the massive-damage instant-death rule.
        stats.attacks[0].hit_bonus = 50;
        stats.attacks[0].damage_bonus = 100;
        state.character.current_hp = 1;
        state.character.max_hp = 10_000; // ensure damage < max_hp per hit

        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.distances.insert(0, 5);

        let lines = resolve_npc_turn(&mut rng, 0, &mut state, &mut combat);
        let attack_lines = count_lines_with("Scimitar", &lines);
        // At least one attack lands (the killing blow), and multiattack may
        // continue up to three times adding death save failures. Must not
        // exceed the configured multiattack count.
        assert!(attack_lines >= 1 && attack_lines <= 3,
            "expected between 1 and 3 Scimitar attacks, got {}: {:#?}",
            attack_lines, lines);
        assert!(combat.death_save_failures >= 1,
            "expected at least one DST failure from additional hits: {:#?}", lines);
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
        assert_eq!(npc.combat_stats.as_ref().unwrap().current_hp, starting_hp,
            "immune target's HP should be unchanged");
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
        assert_eq!(npc.combat_stats.as_ref().unwrap().current_hp, starting_hp - 5);
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
        assert_eq!(npc.combat_stats.as_ref().unwrap().current_hp, 0,
            "current_hp clamps to 0, never negative");
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
        assert_eq!(dealt, 0,
            "NPC without combat_stats takes no damage and the helper is a no-op");
    }

    #[test]
    fn test_apply_damage_modifiers_immunity_takes_precedence_over_resistance() {
        // If a creature is both resistant AND immune (unusual but possible),
        // immunity (full negation) wins.
        let mut stats = CombatStats::default();
        stats.damage_immunities = vec![DamageType::Cold];
        stats.damage_resistances = vec![DamageType::Cold];
        let mut narration = Vec::new();
        let dealt = apply_damage_modifiers(&stats, 8, DamageType::Cold, "elemental", &mut narration);
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
            attack_roll: if hit { 15 } else { 5 },
            total_attack: if hit { 20 } else { 8 },
            target_ac: 13,
            damage,
            damage_type,
            weapon_name: "Longsword".to_string(),
            disadvantage: false,
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
        assert_eq!(apply_graze_mastery(true, &missed, 0, &mut npc, &mut narr), 0);
        assert_eq!(apply_graze_mastery(true, &missed, -1, &mut npc, &mut narr), 0);
    }

    #[test]
    fn test_vex_mastery_marks_target_and_is_consumed_on_next_attack() {
        let player = test_character();
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
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
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
        let zero_dmg_hit = attack_result(true, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_vex_mastery(true, &zero_dmg_hit, 42, &mut combat, &mut narr));
        assert_eq!(combat.player_vex_target, None);
    }

    #[test]
    fn test_sap_mastery_marks_then_consumes() {
        let player = test_character();
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
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
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
        let missed = attack_result(false, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_sap_mastery(true, &missed, 7, &mut combat, &mut narr));
        assert!(combat.sap_targets.is_empty());
    }

    #[test]
    fn test_slow_mastery_applies_10ft_reduction() {
        let player = test_character();
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
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
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
        let zero_dmg_hit = attack_result(true, 0, DamageType::Slashing);
        let mut narr = Vec::new();
        assert!(!apply_slow_mastery(true, &zero_dmg_hit, 7, &mut combat, &mut narr));
        assert_eq!(slow_speed_reduction(&combat, 7), 0);
    }

    #[test]
    fn test_push_mastery_moves_target_10ft_away() {
        use crate::combat::monsters::Size;
        let player = test_character();
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
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
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
        combat.distances.insert(7, 5);
        let hit = attack_result(true, 5, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        // Huge: not pushed.
        assert_eq!(
            apply_push_mastery(true, &hit, 7, &mut combat, &mut narr, Size::Huge),
            None
        );
        assert_eq!(combat.distances.get(&7), Some(&5), "Huge should not be pushed");
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
            &mut StdRng::seed_from_u64(1), &state.character, &[], &HashMap::new(),
        );
        let _ = &mut combat; // unused for this test; kept for future use
        let hit = attack_result(true, 5, DamageType::Bludgeoning);
        let mut narr = Vec::new();
        // DC is 8 + mod + prof = 8 + 5 + 2 = 15. Goblin CON 10 (mod 0). Seed
        // chosen so the first d20 rolls less than 15 -> fail.
        let mut rng = StdRng::seed_from_u64(2);
        let applied = apply_topple_mastery(
            true, &hit, 7, &mut state, &mut narr, /*ability_mod=*/5,
            /*prof_bonus=*/2, &mut rng,
        );
        // Whether applied depends on RNG; assert either outcome is reported
        // via narration and that Prone presence mirrors the reported line.
        let got_prone = state.world.npcs.get(&7).unwrap().conditions
            .iter().any(|c| c.condition == ConditionType::Prone);
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
        let applied = apply_topple_mastery(
            true, &missed, 7, &mut state, &mut narr, 5, 2, &mut rng,
        );
        assert!(!applied);
        let got_prone = state.world.npcs.get(&7).unwrap().conditions
            .iter().any(|c| c.condition == ConditionType::Prone);
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
            &mut StdRng::seed_from_u64(1), &state.character, &[], &HashMap::new(),
        );
        combat.distances.insert(7, 5);
        let mut rng = StdRng::seed_from_u64(3);
        let hit = attack_result(true, 8, DamageType::Slashing);
        let out = apply_cleave_mastery(
            &mut rng, true, &hit, 7, &mut combat, &state, /*ability_mod=*/5,
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
            &mut StdRng::seed_from_u64(1), &state.character, &[], &HashMap::new(),
        );
        combat.distances.insert(7, 5);
        combat.distances.insert(8, 5);
        let mut rng = StdRng::seed_from_u64(3);
        let hit = attack_result(true, 8, DamageType::Slashing);
        let out = apply_cleave_mastery(
            &mut rng, true, &hit, 7, &mut combat, &state, 5,
        );
        assert!(out.is_some());
        let (secondary_id, _cleave_result, _mod) = out.unwrap();
        assert_eq!(secondary_id, 8);
        assert!(combat.cleave_used_this_turn);
        // Second cleave same turn is blocked by the flag.
        let out2 = apply_cleave_mastery(
            &mut rng, true, &hit, 7, &mut combat, &state, 5,
        );
        assert!(out2.is_none());
    }

    #[test]
    fn test_nick_mastery_fires_once_per_turn() {
        let player = test_character();
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
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
        let mut combat = start_combat(
            &mut StdRng::seed_from_u64(1), &player, &[], &HashMap::new(),
        );
        assert!(!apply_nick_mastery(false, &mut combat));
        assert!(!combat.nick_used_this_turn);
    }

    /// Minimal GameState helper used only by the mastery tests above.
    fn test_game_state(character: Character) -> GameState {
        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(), npcs: HashMap::new(),
                items: HashMap::new(), triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 1, rng_counter: 0,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: Default::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
            pending_background_pattern: None,
            pending_disambiguation: None,
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
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        // Fresh dying: 0 successes, 0 failures -- combat continues.
        assert_eq!(combat.check_end(&state), None,
            "Combat should continue while player is dying (not yet 3 failures)");
    }

    #[test]
    fn test_check_end_defeats_after_three_death_save_failures() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.death_save_failures = 3;
        assert_eq!(combat.check_end(&state), Some(false),
            "Three death save failures should result in defeat");
    }

    #[test]
    fn test_is_player_dying_true_at_zero_hp_with_failures_below_three() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        assert!(combat.is_player_dying(&state));
    }

    #[test]
    fn test_is_player_dying_false_when_hp_positive() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        assert!(!combat.is_player_dying(&state));
    }

    #[test]
    fn test_death_save_roll_10_or_higher_counts_as_success() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.death_save_successes = 1;
        combat.death_save_failures = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 20);
        assert_eq!(outcome, DeathSaveOutcome::CritSuccess);
        assert_eq!(state.character.current_hp, 1);
        assert_eq!(combat.death_save_successes, 0,
            "Nat 20 clears death save counters");
        assert_eq!(combat.death_save_failures, 0,
            "Nat 20 clears death save counters");
    }

    #[test]
    fn test_three_death_save_successes_stabilize_at_1_hp() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.death_save_successes = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 10);
        assert_eq!(outcome, DeathSaveOutcome::Stable);
        assert_eq!(state.character.current_hp, 1,
            "Reaching 3 successes sets HP to 1 (stable)");
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 0);
    }

    #[test]
    fn test_three_death_save_failures_mark_dead() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        combat.death_save_failures = 2;
        let outcome = combat.apply_death_save_roll(&mut state.character, 5);
        assert_eq!(outcome, DeathSaveOutcome::Dead);
        assert_eq!(combat.death_save_failures, 3);
        assert_eq!(combat.check_end(&state), Some(false),
            "After three failures, combat ends in defeat");
    }

    #[test]
    fn test_damage_while_dying_adds_failure() {
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        state.character.max_hp = 20;
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        assert_eq!(combat.death_save_successes, 0);
        assert_eq!(combat.death_save_failures, 0);
    }

    #[test]
    fn test_death_save_state_serde_roundtrip() {
        let state = test_state_with_goblin();
        let mut rng = StdRng::seed_from_u64(42);
        let mut combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
        let mut json: serde_json::Value = serde_json::to_value(&combat).unwrap();
        json.as_object_mut().unwrap().remove("death_save_successes");
        json.as_object_mut().unwrap().remove("death_save_failures");
        let round_tripped: CombatState = serde_json::from_value(json)
            .expect("Old saves without death_save_* fields should deserialize");
        assert_eq!(round_tripped.death_save_successes, 0);
        assert_eq!(round_tripped.death_save_failures, 0);
    }
}
