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

/// Identifies a combatant in initiative order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Combatant {
    Player,
    Npc(NpcId),
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
    pub player_action_used: bool,
    /// NPCs that are dodging (NpcId -> true until their next turn).
    pub npc_dodging: HashMap<NpcId, bool>,
    /// NPCs that are disengaging this turn.
    pub npc_disengaging: HashMap<NpcId, bool>,
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
                    .unwrap_or(true)
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
                    // Reset player turn state
                    self.player_movement_remaining = state.character.speed;
                    self.player_dodging = false;
                    self.player_disengaging = false;
                    self.player_action_used = false;
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
        player_action_used: false,
        npc_dodging: HashMap::new(),
        npc_disengaging: HashMap::new(),
    }
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

    let is_unarmed = damage_dice == 0;

    if is_unarmed {
        // Unarmed strike: always hits, 1 + STR mod (min 1)
        let str_mod = player.ability_modifier(Ability::Strength);
        let damage = (1 + str_mod).max(1);
        return AttackResult {
            hit: true,
            natural_20: false,
            natural_1: false,
            attack_roll: 0,
            total_attack: 0,
            target_ac,
            damage,
            damage_type: DamageType::Bludgeoning,
            weapon_name,
            disadvantage: false,
        };
    }

    let is_finesse = properties & FINESSE != 0;
    let is_thrown = properties & THROWN != 0;
    let is_versatile = properties & VERSATILE != 0;
    let _is_reach = properties & REACH != 0;
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
    if ranged {
        if hostile_within_5ft {
            disadvantage = true;
        }
        if distance > range_normal as u32 && distance <= range_long as u32 {
            disadvantage = true; // Long range
        }
    }

    // Roll attack
    let roll1 = roll_d20(rng);
    let roll2 = roll_d20(rng);
    let attack_roll = if disadvantage { roll1.min(roll2) } else { roll1 };

    let natural_20 = attack_roll == 20;
    let natural_1 = attack_roll == 1;

    let total_attack = attack_roll + ability_mod + prof_bonus;
    let hit = if natural_1 { false } else if natural_20 { true } else { total_attack >= target_ac };

    let damage = if hit {
        // Determine die to use
        let actual_die = if is_versatile && off_hand_free && versatile_die > 0 {
            versatile_die
        } else {
            damage_die
        };

        let dice_count = if natural_20 { damage_dice * 2 } else { damage_dice };
        let dice_total: i32 = roll_dice(rng, dice_count, actual_die).iter().sum();
        (dice_total + ability_mod).max(1)
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
) -> AttackResult {
    let mut disadvantage = false;
    if player_dodging {
        disadvantage = true;
    }

    let is_ranged = attack.reach == 0 && attack.range_normal > 0;
    if is_ranged && distance <= 5 {
        disadvantage = true; // Ranged attack in melee
    }
    if is_ranged && distance > attack.range_normal as u32 {
        disadvantage = true; // Long range
    }

    let roll1 = roll_d20(rng);
    let roll2 = roll_d20(rng);
    let attack_roll = if disadvantage { roll1.min(roll2) } else { roll1 };

    let natural_20 = attack_roll == 20;
    let natural_1 = attack_roll == 1;

    let total_attack = attack_roll + attack.hit_bonus;
    let hit = if natural_1 { false } else if natural_20 { true } else { total_attack >= player_ac };

    let damage = if hit {
        let dice_count = if natural_20 { attack.damage_dice * 2 } else { attack.damage_dice };
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
        disadvantage,
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
    let result = resolve_npc_attack(rng, melee_attack, player_ac, false, distance);
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

    // Priority: melee if in range -> ranged if in range -> move toward player

    // Check for melee attack
    let melee_attack = npc_attacks.iter().find(|a| a.reach > 0 && distance <= a.reach as u32);
    if let Some(attack) = melee_attack {
        let player_ac = crate::equipment::calculate_ac(&state.character, &state.world.items);
        let player_dodging = combat.player_dodging;
        let result = resolve_npc_attack(rng, attack, player_ac, player_dodging, distance);

        if result.hit {
            state.character.current_hp -= result.damage;
            if result.natural_20 {
                lines.push(format!("{} attacks with {} -- CRITICAL HIT! {} {} damage!",
                    npc_name, result.weapon_name, result.damage, result.damage_type));
            } else {
                lines.push(format!("{} attacks with {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                    npc_name, result.weapon_name, result.attack_roll,
                    attack.hit_bonus, result.total_attack, player_ac,
                    result.damage, result.damage_type));
            }
        } else if result.natural_1 {
            lines.push(format!("{} attacks with {} -- natural 1, miss!", npc_name, result.weapon_name));
        } else {
            lines.push(format!("{} attacks with {} ({}+{}={} vs AC {}) -- miss.",
                npc_name, result.weapon_name, result.attack_roll,
                attack.hit_bonus, result.total_attack, player_ac));
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
        let result = resolve_npc_attack(rng, attack, player_ac, player_dodging, distance);

        if result.hit {
            state.character.current_hp -= result.damage;
            if result.natural_20 {
                lines.push(format!("{} fires {} -- CRITICAL HIT! {} {} damage!",
                    npc_name, result.weapon_name, result.damage, result.damage_type));
            } else {
                lines.push(format!("{} fires {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                    npc_name, result.weapon_name, result.attack_roll,
                    attack.hit_bonus, result.total_attack, player_ac,
                    result.damage, result.damage_type));
            }
        } else if result.natural_1 {
            lines.push(format!("{} fires {} -- natural 1, miss!", npc_name, result.weapon_name));
        } else {
            lines.push(format!("{} fires {} ({}+{}={} vs AC {}) -- miss.",
                npc_name, result.weapon_name, result.attack_roll,
                attack.hit_bonus, result.total_attack, player_ac));
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
        if !combat.player_action_used {
            lines.push("Action available. Commands: attack <target>, dodge, disengage, dash".to_string());
        } else {
            lines.push("Action used. You can still move (approach/retreat).".to_string());
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
            let result = resolve_npc_attack(&mut rng, &attack, 15, false, 5);
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
            let result = resolve_npc_attack(&mut rng, &attack, 30, false, 5);
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
            let result = resolve_npc_attack(&mut rng, &attack, 1, false, 5);
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
            let result = resolve_npc_attack(&mut rng, &attack, 10, false, 5);
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
            let dodge = resolve_npc_attack(&mut rng1, &attack, 15, true, 5);
            let normal = resolve_npc_attack(&mut rng2, &attack, 15, false, 5);
            if dodge.hit { dodge_hits += 1; }
            if normal.hit { normal_hits += 1; }
        }
        assert!(dodge_hits < normal_hits, "Dodging should reduce hit rate: dodge={}, normal={}", dodge_hits, normal_hits);
    }

    #[test]
    fn test_unarmed_strike() {
        let mut rng = StdRng::seed_from_u64(42);
        let player = test_character();
        let items = HashMap::new();
        let result = resolve_player_attack(&mut rng, &player, 15, false, None, &items, 5, true, false);
        assert!(result.hit, "Unarmed always hits");
        // STR 16+1(human)=17, mod +3, damage = 1+3 = 4
        assert_eq!(result.damage, 4);
        assert_eq!(result.weapon_name, "Unarmed");
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
            }),
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
}
