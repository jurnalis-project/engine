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
}

impl CombatState {
    /// Check if combat is over. Returns Some(true) for victory, Some(false) for defeat.
    pub fn check_end(&self, state: &GameState) -> Option<bool> {
        if state.character.current_hp <= 0 {
            return Some(false); // defeat
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
) -> AttackResult {
    let (weapon_name, damage_dice, damage_die, damage_type, properties, versatile_die, range_normal, range_long) =
        match weapon_id.and_then(|id| items.get(&id)) {
            Some(item) => match &item.item_type {
                ItemType::Weapon { damage_dice, damage_die, damage_type, properties, versatile_die, range_normal, range_long, .. } => {
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

    let (npc_name, npc_speed, npc_attacks, _npc_ac) = {
        let npc = match state.world.npcs.get(&npc_id) {
            Some(n) => n,
            None => return lines,
        };
        let stats = match npc.combat_stats.as_ref() {
            Some(s) if s.current_hp > 0 => s,
            _ => return lines,
        };
        (npc.name.clone(), stats.speed, stats.attacks.clone(), stats.ac)
    };

    let distance = *combat.distances.get(&npc_id).unwrap_or(&30);

    // Get NPC and conditions reference for attack resolution
    let npc_ref = match state.world.npcs.get(&npc_id) {
        Some(n) => n,
        None => return lines,
    };
    let npc_conditions = &npc_ref.conditions;
    let player_conditions = &state.character.conditions;

    // Priority: melee if in range -> ranged if in range -> move toward player

    // Orchestrator-side grappled disadvantage: if the NPC is grappled by
    // someone other than the player, attacking the player is at disadvantage.
    let extra_disadvantage = conditions::grappled_attack_disadvantage(
        npc_conditions,
        &state.character.name,
    );

    // Check for melee attack
    let melee_attack = npc_attacks.iter().find(|a| a.reach > 0 && distance <= a.reach as u32);
    if let Some(attack) = melee_attack {
        let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
        let player_dodging = combat.player_dodging;
        let result = resolve_npc_attack(rng, attack, player_ac, player_dodging, distance, npc_conditions, player_conditions, extra_disadvantage);
        let disadv = if result.disadvantage { " (with disadvantage)" } else { "" };

        if result.hit {
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
        } else if result.natural_1 {
            lines.push(format!("{} attacks with {} -- natural 1, miss!", npc_name, result.weapon_name));
        } else {
            lines.push(format!("{} attacks with {} ({}+{}={} vs AC {}){} -- miss.",
                npc_name, result.weapon_name, result.attack_roll,
                attack.hit_bonus, result.total_attack, player_ac, disadv));
        }
        return lines;
    }

    // Check for ranged attack
    let ranged_attack = npc_attacks.iter().find(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    });
    if let Some(attack) = ranged_attack {
        let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
        let player_dodging = combat.player_dodging;
        let result = resolve_npc_attack(rng, attack, player_ac, player_dodging, distance, npc_conditions, player_conditions, extra_disadvantage);
        let disadv = if result.disadvantage { " (with disadvantage)" } else { "" };

        if result.hit {
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
        } else if result.natural_1 {
            lines.push(format!("{} fires {} -- natural 1, miss!", npc_name, result.weapon_name));
        } else {
            lines.push(format!("{} fires {} ({}+{}={} vs AC {}){} -- miss.",
                npc_name, result.weapon_name, result.attack_roll,
                attack.hit_bonus, result.total_attack, player_ac, disadv));
        }
        return lines;
    }

    // Move toward player
    let move_amount = npc_speed as u32;
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
                    state.character.current_hp -= result.damage;
                    lines.push(format!("{} makes an opportunity attack with {} -- hit for {} {} damage!",
                        npc_name, result.weapon_name, result.damage, result.damage_type));
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
            let result = resolve_player_attack(&mut rng, &player, 100, false, None, &items, 5, true, false, &[], false);
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
            let result = resolve_player_attack(&mut rng, &player, 1, false, None, &items, 5, true, false, &[], false);
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
            let normal = resolve_player_attack(&mut rng1, &player, 15, false, None, &items, 5, true, false, &[], false);
            let poisoned = resolve_player_attack(&mut rng2, &poisoned_player, 15, false, None, &items, 5, true, false, &[], false);
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
        let mut state = test_state_with_goblin();
        state.character.current_hp = 0;
        let mut rng = StdRng::seed_from_u64(42);
        let combat = start_combat(&mut rng, &state.character, &[0], &state.world.npcs);
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
                false,
            );
            let res_neu = resolve_player_attack(
                &mut rng2, &state_neu.character, target_ac, false, Some(9999),
                &state_neu.world.items, distance, true, false, &[],
                false,
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
            };
            state.world.items.insert(9999, club);
            state.character.inventory.push(9999);
            state.character.equipped.main_hand = Some(9999);

            let mut rng1 = StdRng::seed_from_u64(seed);
            let mut rng2 = StdRng::seed_from_u64(seed);

            let res_disadv = resolve_player_attack(
                &mut rng1, &state.character, 15, false, Some(9999),
                &state.world.items, 5, true, false, &[], true,
            );
            let res_neu = resolve_player_attack(
                &mut rng2, &state.character, 15, false, Some(9999),
                &state.world.items, 5, true, false, &[], false,
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
}
