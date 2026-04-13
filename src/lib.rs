pub mod types;
pub mod rules;
pub mod character;
pub mod state;
pub mod parser;
pub mod world;
pub mod narration;
pub mod equipment;
pub mod output;
pub mod combat;
pub mod conditions;
pub mod spells;

use std::collections::{HashMap, HashSet};
use rand::SeedableRng;
use rand::rngs::StdRng;

use output::GameOutput;
use parser::Command;
use parser::resolver::{self, ResolveResult};
use state::{GameState, GamePhase, CreationStep, SAVE_VERSION};
use character::{race::Race, class::Class, STANDARD_ARRAY, generate_random_scores};
#[cfg(test)]
use character::create_character;
use types::{Ability, Skill};

pub fn new_game(seed: u64, ironman_mode: bool) -> GameOutput {
    let state = GameState {
        version: SAVE_VERSION.to_string(),
        character: character::create_character(
            "Unnamed".to_string(),
            Race::Human,
            Class::Fighter,
            HashMap::new(),
            Vec::new(),
        ),
        current_location: 0,
        discovered_locations: HashSet::new(),
        world: state::WorldState {
            locations: HashMap::new(),
            npcs: HashMap::new(),
            items: HashMap::new(),
            triggers: HashMap::new(),
            triggered: HashSet::new(),
        },
        log: Vec::new(),
        rng_seed: seed,
        rng_counter: 0,
        game_phase: GamePhase::CharacterCreation(CreationStep::ChooseRace),
        active_combat: None,
        ironman_mode,
        progress: state::ProgressState::default(),
    };

    let state_json = serde_json::to_string(&state).unwrap();
    let text = vec![
        "=== Welcome to Jurnalis ===".to_string(),
        String::new(),
        "Let's create your character.".to_string(),
        String::new(),
        "Choose your race:".to_string(),
        "  1. Human (+1 to all abilities)".to_string(),
        "  2. Elf (+2 DEX, Darkvision, Fey Ancestry)".to_string(),
        "  3. Dwarf (+2 CON, Darkvision, Dwarven Resilience)".to_string(),
    ];

    GameOutput::new(text, state_json, true)
}

pub fn process_input(state_json: &str, input: &str) -> GameOutput {
    let mut state: GameState = match serde_json::from_str(state_json) {
        Ok(s) => s,
        Err(e) => {
            return GameOutput::message(
                format!("Error loading state: {}", e),
                state_json.to_string(),
            );
        }
    };

    let old_state_json = state_json.to_string();

    if state.character.current_hp <= 0 {
        let command = parser::parse(input);
        if command == Command::NewGame {
            let new_seed = state.rng_seed.wrapping_add(state.rng_counter);
            return new_game(new_seed, state.ironman_mode);
        }
        let text = vec![
            "=== GAME OVER ===".to_string(),
            "You have been defeated.".to_string(),
            "Load a previous save or type `new game` to start over.".to_string(),
        ];
        return GameOutput::new(text, old_state_json, false);
    }

    let result = if state.active_combat.is_some() {
        handle_combat(&mut state, input)
    } else {
        match state.game_phase {
            GamePhase::CharacterCreation(step) => handle_creation(&mut state, input, step),
            GamePhase::Exploration => handle_exploration(&mut state, input),
        }
    };

    let new_state_json = serde_json::to_string(&state).unwrap();
    let state_changed = new_state_json != old_state_json;
    GameOutput::new(result, new_state_json, state_changed)
}

fn handle_creation(state: &mut GameState, input: &str, step: CreationStep) -> Vec<String> {
    let input = input.trim();
    match step {
        CreationStep::ChooseRace => {
            let race = match input {
                "1" | "human" => Race::Human,
                "2" | "elf" => Race::Elf,
                "3" | "dwarf" => Race::Dwarf,
                _ => return vec!["Please choose 1 (Human), 2 (Elf), or 3 (Dwarf).".to_string()],
            };
            state.character.race = race;
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseClass);
            vec![
                format!("Race: {}. Now choose your class:", race),
                "  1. Fighter (d10 HP, STR/CON saves)".to_string(),
                "  2. Rogue (d8 HP, DEX/INT saves, 4 skills)".to_string(),
                "  3. Wizard (d6 HP, INT/WIS saves)".to_string(),
            ]
        }
        CreationStep::ChooseClass => {
            let class = match input {
                "1" | "fighter" => Class::Fighter,
                "2" | "rogue" => Class::Rogue,
                "3" | "wizard" => Class::Wizard,
                _ => return vec!["Please choose 1 (Fighter), 2 (Rogue), or 3 (Wizard).".to_string()],
            };
            state.character.class = class;
            state.character.save_proficiencies = class.saving_throw_proficiencies();
            // Set spell fields based on class
            match class {
                Class::Wizard => {
                    state.character.spell_slots_max = vec![2];
                    state.character.spell_slots_remaining = vec![2];
                    state.character.known_spells = vec![
                        "Fire Bolt".to_string(),
                        "Prestidigitation".to_string(),
                        "Magic Missile".to_string(),
                        "Burning Hands".to_string(),
                        "Sleep".to_string(),
                        "Shield".to_string(),
                    ];
                }
                _ => {
                    state.character.spell_slots_max = Vec::new();
                    state.character.spell_slots_remaining = Vec::new();
                    state.character.known_spells = Vec::new();
                }
            }
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseAbilityMethod);
            vec![
                format!("Class: {}. Choose ability score method:", class),
                "  1. Standard Array (15, 14, 13, 12, 10, 8)".to_string(),
                "  2. Random (4d6 drop lowest)".to_string(),
                "  3. Point Buy (27 points, scores 8-15)".to_string(),
            ]
        }
        CreationStep::ChooseAbilityMethod => {
            match input {
                "1" | "standard" => {
                    state.game_phase = GamePhase::CharacterCreation(CreationStep::AssignAbilities);
                    let scores = STANDARD_ARRAY;
                    // Store the unassigned scores temporarily in the log
                    state.log = scores.iter().map(|s| s.to_string()).collect();
                    vec![
                        format!("Scores to assign: {:?}", scores),
                        "Assign scores to abilities. Enter six numbers (one per ability):".to_string(),
                        "Format: STR DEX CON INT WIS CHA".to_string(),
                        "Example: 15 14 13 12 10 8".to_string(),
                    ]
                }
                "2" | "random" => {
                    let mut rng = StdRng::seed_from_u64(state.rng_seed + state.rng_counter);
                    state.rng_counter += 1;
                    let scores = generate_random_scores(&mut rng);
                    state.game_phase = GamePhase::CharacterCreation(CreationStep::AssignAbilities);
                    state.log = scores.iter().map(|s| s.to_string()).collect();
                    vec![
                        format!("Rolled scores: {:?}", scores),
                        "Assign scores to abilities. Enter six numbers (one per ability):".to_string(),
                        "Format: STR DEX CON INT WIS CHA".to_string(),
                        format!("Example: {} {} {} {} {} {}", scores[0], scores[1], scores[2], scores[3], scores[4], scores[5]),
                    ]
                }
                "3" | "point buy" | "pointbuy" => {
                    state.game_phase = GamePhase::CharacterCreation(CreationStep::PointBuy);
                    vec![
                        "Point Buy: 27 points to spend. Scores range from 8 to 15.".to_string(),
                        "Cost: 8=0, 9=1, 10=2, 11=3, 12=4, 13=5, 14=7, 15=9".to_string(),
                        String::new(),
                        "Enter six scores for STR DEX CON INT WIS CHA.".to_string(),
                        "Example: 15 14 13 12 10 8 (total: 27 points)".to_string(),
                    ]
                }
                _ => vec!["Please choose 1 (Standard Array), 2 (Random), or 3 (Point Buy).".to_string()],
            }
        }
        CreationStep::PointBuy => {
            let values: Vec<i32> = input.split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() != 6 {
                return vec!["Please enter exactly 6 numbers separated by spaces.".to_string()];
            }

            let cost_table: std::collections::HashMap<i32, i32> = [
                (8, 0), (9, 1), (10, 2), (11, 3), (12, 4), (13, 5), (14, 7), (15, 9),
            ].into_iter().collect();

            let mut total_cost = 0;
            for &v in &values {
                match cost_table.get(&v) {
                    Some(&cost) => total_cost += cost,
                    None => return vec![format!("Invalid score: {}. Scores must be between 8 and 15.", v)],
                }
            }

            if total_cost != 27 {
                return vec![format!("Total cost is {} points (must be exactly 27). Adjust your scores.", total_cost)];
            }

            let abilities = Ability::all();
            for (i, &ability) in abilities.iter().enumerate() {
                state.character.ability_scores.insert(ability, values[i]);
            }

            // Apply racial bonuses
            for (ability, bonus) in state.character.race.ability_bonuses() {
                *state.character.ability_scores.entry(ability).or_insert(10) += bonus;
            }

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseSkills);

            let class = state.character.class;
            let choices = class.skill_choices();
            let count = class.skill_choice_count();

            let mut lines = vec![
                "Ability scores assigned!".to_string(),
                String::new(),
                format!("Choose {} skill proficiencies from:", count),
            ];
            for (i, skill) in choices.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, skill));
            }
            lines.push(format!("Enter {} numbers separated by spaces.", count));
            lines
        }
        CreationStep::AssignAbilities => {
            let available: Vec<i32> = state.log.iter()
                .filter_map(|s| s.parse().ok())
                .collect();

            let values: Vec<i32> = input.split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if values.len() != 6 {
                return vec!["Please enter exactly 6 numbers separated by spaces.".to_string()];
            }

            // Verify all values are from the available set
            let mut remaining = available.clone();
            for &v in &values {
                if let Some(pos) = remaining.iter().position(|&x| x == v) {
                    remaining.remove(pos);
                } else {
                    return vec![format!("Invalid assignment. Available scores: {:?}", available)];
                }
            }

            let abilities = Ability::all();
            for (i, &ability) in abilities.iter().enumerate() {
                state.character.ability_scores.insert(ability, values[i]);
            }

            // Apply racial bonuses
            for (ability, bonus) in state.character.race.ability_bonuses() {
                *state.character.ability_scores.entry(ability).or_insert(10) += bonus;
            }

            state.log.clear();
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseSkills);

            let class = state.character.class;
            let choices = class.skill_choices();
            let count = class.skill_choice_count();

            let mut lines = vec![
                "Ability scores assigned!".to_string(),
                String::new(),
                format!("Choose {} skill proficiencies from:", count),
            ];
            for (i, skill) in choices.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, skill));
            }
            lines.push(format!("Enter {} numbers separated by spaces.", count));
            lines
        }
        CreationStep::ChooseSkills => {
            let class = state.character.class;
            let choices = class.skill_choices();
            let count = class.skill_choice_count();

            let indices: Vec<usize> = input.split_whitespace()
                .filter_map(|s| s.parse::<usize>().ok())
                .collect();

            if indices.len() != count {
                return vec![format!("Please choose exactly {} skills.", count)];
            }

            let mut skills = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for &idx in &indices {
                if idx < 1 || idx > choices.len() {
                    return vec![format!("Invalid choice: {}. Pick from 1-{}.", idx, choices.len())];
                }
                if !seen.insert(idx) {
                    return vec![format!("Duplicate choice: {}. Each skill must be different.", idx)];
                }
                skills.push(choices[idx - 1]);
            }

            state.character.skill_proficiencies = skills;
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseName);

            vec!["Skills chosen! Enter your character's name:".to_string()]
        }
        CreationStep::ChooseName => {
            let name = input.trim().to_string();
            if name.is_empty() {
                return vec!["Please enter a name.".to_string()];
            }

            state.character.name = name.clone();
            state.character.traits = state.character.race.traits().iter().map(|s| s.to_string()).collect();

            // Calculate HP
            let con_mod = state.character.ability_modifier(Ability::Constitution);
            state.character.max_hp = character::calculate_hp(state.character.class, con_mod, 1);
            state.character.current_hp = state.character.max_hp;
            state.character.level = 1;
            state.character.speed = state.character.race.speed();

            // Generate world
            let mut rng = StdRng::seed_from_u64(state.rng_seed);
            let world = world::generate_world(&mut rng, 15);
            state.world = world;
            state.current_location = 0;
            state.discovered_locations.insert(0);

            state.game_phase = GamePhase::Exploration;
            state.log.clear();

            let mut lines = vec![
                format!("{} the {} {} is ready for adventure!", name, state.character.race, state.character.class),
                String::new(),
            ];

            // Narrate entering the first location
            let mut rng = StdRng::seed_from_u64(state.rng_seed + 1000);
            if let Some(loc) = state.world.locations.get(&state.current_location) {
                lines.extend(narration::narrate_enter_location(&mut rng, loc, state));
            }

            lines
        }
    }
}

const NPC_TAG: usize = 1 << 31;

fn npc_candidates(state: &GameState) -> Vec<(usize, String)> {
    let loc = match state.world.locations.get(&state.current_location) {
        Some(loc) => loc,
        None => return Vec::new(),
    };
    loc.npcs.iter()
        .filter_map(|&id| state.world.npcs.get(&id).map(|npc| ((id as usize) | NPC_TAG, npc.name.clone())))
        .collect()
}

fn room_item_candidates(state: &GameState) -> Vec<(usize, String)> {
    let loc = match state.world.locations.get(&state.current_location) {
        Some(loc) => loc,
        None => return Vec::new(),
    };
    loc.items.iter()
        .filter_map(|&id| state.world.items.get(&id).map(|item| (id, item)))
        .filter(|(_, item)| !item.carried_by_player)
        .map(|(id, item)| (id as usize, item.name.clone()))
        .collect()
}

fn inventory_item_candidates(state: &GameState) -> Vec<(usize, String)> {
    state.character.inventory.iter()
        .filter_map(|&id| state.world.items.get(&id).map(|item| (id as usize, item.name.clone())))
        .collect()
}

fn build_combat_npc_candidates(combat: &combat::CombatState, state: &GameState) -> Vec<(usize, String)> {
    combat.initiative_order.iter()
        .filter_map(|(c, _)| {
            if let combat::Combatant::Npc(id) = c {
                let npc = state.world.npcs.get(id)?;
                let stats = npc.combat_stats.as_ref()?;
                if stats.current_hp > 0 {
                    Some((*id as usize, npc.name.clone()))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .collect()
}

fn build_spell_targets(combat: &combat::CombatState, state: &GameState) -> Vec<spells::SpellTarget> {
    combat.initiative_order.iter()
        .filter_map(|(c, _)| {
            if let combat::Combatant::Npc(id) = c {
                let npc = state.world.npcs.get(id)?;
                let stats = npc.combat_stats.as_ref()?;
                if stats.current_hp <= 0 {
                    return None;
                }
                let distance = combat.distances.get(id).copied().unwrap_or(30);
                Some(spells::SpellTarget {
                    id: *id,
                    name: npc.name.clone(),
                    ac: stats.ac,
                    current_hp: stats.current_hp,
                    ability_scores: stats.ability_scores.clone(),
                    proficiency_bonus: stats.proficiency_bonus,
                    save_proficiencies: Vec::new(), // NPCs don't have save proficiency tracking in MVP
                    distance,
                })
            } else {
                None
            }
        })
        .collect()
}

fn handle_exploration(state: &mut GameState, input: &str) -> Vec<String> {
    let command = parser::parse(input);
    let mut rng = StdRng::seed_from_u64(state.rng_seed + state.rng_counter);
    state.rng_counter += 1;

    match command {
        Command::Look(target) => {
            if let Some(target) = target {
                // Look at specific thing — search room NPCs, room items, and inventory items.
                let mut owned_candidates = npc_candidates(state);
                owned_candidates.extend(room_item_candidates(state));
                owned_candidates.extend(inventory_item_candidates(state));

                let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                    .map(|(id, name)| (*id, name.as_str()))
                    .collect();

                return match resolver::resolve_target(&target, &candidates) {
                    ResolveResult::Found(id) => {
                        if id & NPC_TAG != 0 {
                            let npc_id = (id & !NPC_TAG) as u32;
                            if let Some(npc) = state.world.npcs.get(&npc_id) {
                                return vec![format!("{} — {} ({})", npc.name, npc.role_description(), npc.disposition_description())];
                            }
                        } else {
                            let item_id = id as u32;
                            if let Some(item) = state.world.items.get(&item_id) {
                                return vec![format!("{}: {}", item.name, item.description)];
                            }
                        }
                        vec![format!("You don't see any \"{}\" here.", target)]
                    }
                    ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                    ResolveResult::NotFound => vec![format!("You don't see any \"{}\" here.", target)],
                };
            } else {
                let loc = state.world.locations.get(&state.current_location).cloned();
                match loc {
                    Some(loc) => narration::narrate_look(&mut rng, &loc, state),
                    None => vec!["You are nowhere.".to_string()],
                }
            }
        }
        Command::Go(direction) => {
            let next = state.world.locations
                .get(&state.current_location)
                .and_then(|loc| loc.exits.get(&direction).copied());

            match next {
                Some(next_id) => {
                    state.current_location = next_id;
                    state.discovered_locations.insert(next_id);

                    let mut lines = Vec::new();
                    lines.push(format!("You go {}.", direction));
                    lines.push(String::new());

                    // Check triggers
                    let loc = state.world.locations.get(&next_id).cloned();
                    if let Some(loc) = &loc {
                        let trigger_ids: Vec<_> = loc.triggers.iter()
                            .filter(|id| !state.world.triggered.contains(id))
                            .copied()
                            .collect();

                        for tid in trigger_ids {
                            if let Some(trigger) = state.world.triggers.get(&tid).cloned() {
                                let check_result = match &trigger.trigger_type {
                                    state::TriggerType::SkillCheck(skill) => {
                                        let result = rules::checks::skill_check(
                                            &mut rng,
                                            *skill,
                                            &state.character.ability_scores,
                                            &state.character.skill_proficiencies,
                                            state.character.proficiency_bonus(),
                                            trigger.dc,
                                            false, false,
                                        );
                                        let narration = narration::narrate_skill_check(&mut rng, &skill.to_string(), &result);
                                        Some((result.success, narration))
                                    }
                                    state::TriggerType::SavingThrow(ability) => {
                                        let score = state.character.ability_scores.get(ability).copied().unwrap_or(10);
                                        let is_prof = state.character.is_proficient_in_save(*ability);
                                        let result = rules::checks::ability_check(
                                            &mut rng, score,
                                            state.character.proficiency_bonus(),
                                            is_prof, trigger.dc, false, false,
                                        );
                                        let narration = narration::narrate_skill_check(&mut rng, &format!("{} save", ability), &result);
                                        Some((result.success, narration))
                                    }
                                    state::TriggerType::PassivePerception => {
                                        let score = state.character.ability_scores.get(&Ability::Wisdom).copied().unwrap_or(10);
                                        let is_prof = state.character.is_proficient_in_skill(Skill::Perception);
                                        let passive = rules::checks::passive_check(score, state.character.proficiency_bonus(), is_prof);
                                        let success = passive >= trigger.dc;
                                        None // Passive checks are silent on failure
                                            .or_else(|| if success {
                                                Some((true, format!("[Passive Perception {} vs DC {} — noticed!]", passive, trigger.dc)))
                                            } else {
                                                None
                                            })
                                    }
                                };

                                if let Some((success, narration)) = check_result {
                                    lines.push(narration);
                                    if success {
                                        lines.push(trigger.success_text.clone());
                                    } else {
                                        lines.push(trigger.failure_text.clone());
                                        if trigger.damage_on_failure > 0 {
                                            state.character.current_hp -= trigger.damage_on_failure;
                                            lines.push(format!("You take {} damage!", trigger.damage_on_failure));
                                            if state.character.current_hp <= 0 {
                                                state.character.current_hp = 0;
                                                lines.push("You have been defeated.".to_string());
                                            }
                                        }
                                    }
                                    if trigger.one_shot {
                                        state.world.triggered.insert(tid);
                                    }
                                }
                            }
                        }
                    }

                    if let Some(loc) = loc {
                        lines.extend(narration::narrate_enter_location(&mut rng, &loc, state));

                        // Check for hostile NPCs to trigger combat
                        let hostile_ids: Vec<u32> = loc.npcs.iter()
                            .filter_map(|&id| {
                                state.world.npcs.get(&id)
                                    .filter(|npc| npc.disposition == state::Disposition::Hostile)
                                    .filter(|npc| npc.combat_stats.is_some())
                                    .filter(|npc| npc.combat_stats.as_ref().unwrap().current_hp > 0)
                                    .map(|_| id)
                            })
                            .collect();

                        if !hostile_ids.is_empty() {
                            let combat_state = combat::start_combat(
                                &mut rng, &state.character, &hostile_ids, &state.world.npcs,
                            );
                            lines.push(String::new());
                            lines.extend(combat::format_initiative(&combat_state, state));

                            // Check if it's the player's turn first
                            if combat_state.is_player_turn() {
                                lines.push(String::new());
                                lines.extend(combat::format_enemy_summary(state, &combat_state));
                                lines.push("Your turn! Use: attack <target>, approach <target>, retreat, dodge, disengage, dash".to_string());
                            } else {
                                // Process NPC turns before the player's first turn
                                state.active_combat = Some(combat_state);
                                let npc_lines = process_npc_turns(state, &mut rng);
                                lines.extend(npc_lines);
                                if let Some(ref combat) = state.active_combat {
                                    if let Some(victory) = combat.check_end(state) {
                                        lines.extend(end_combat(state, victory));
                                    } else {
                                        lines.push(String::new());
                                        lines.extend(combat::format_enemy_summary(state, combat));
                                        lines.push("Your turn! Use: attack <target>, approach <target>, retreat, dodge, disengage, dash".to_string());
                                    }
                                }
                                return lines;
                            }
                            state.active_combat = Some(combat_state);
                        }
                    }
                    lines
                }
                None => vec![narration::templates::NO_EXIT.replace("{direction}", &direction.to_string())],
            }
        }
        Command::Talk(name) => {
            let owned_candidates = npc_candidates(state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, n)| (*id, n.as_str()))
                .collect();

            match resolver::resolve_target(&name, &candidates) {
                ResolveResult::Found(id) => {
                    let npc_id = (id & !NPC_TAG) as u32;
                    if let Some(npc) = state.world.npcs.get(&npc_id) {
                        npc.generate_dialogue(&mut rng)
                    } else {
                        vec![narration::templates::NPC_NOT_FOUND.replace("{name}", &name)]
                    }
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![narration::templates::NPC_NOT_FOUND.replace("{name}", &name)],
            }
        }
        Command::Take(item_name) => {
            let owned_candidates = room_item_candidates(state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&item_name, &candidates) {
                ResolveResult::Found(id) => {
                    let item_id = id as u32;
                    let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| item_name.clone());
                    state.world.items.get_mut(&item_id).unwrap().carried_by_player = true;
                    state.world.items.get_mut(&item_id).unwrap().location = None;
                    state.character.inventory.push(item_id);
                    if let Some(loc) = state.world.locations.get_mut(&state.current_location) {
                        loc.items.retain(|&id| id != item_id);
                    }
                    vec![narration::templates::TAKE_ITEM.replace("{item}", &name)]
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![narration::templates::ITEM_NOT_FOUND.replace("{item}", &item_name)],
            }
        }
        Command::Drop(item_name) => {
            let owned_candidates = inventory_item_candidates(state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&item_name, &candidates) {
                ResolveResult::Found(id) => {
                    let item_id = id as u32;
                    let current_location = state.current_location;
                    state.character.inventory.retain(|&id| id != item_id);
                    // Clear equipment slot if dropping an equipped item
                    if state.character.equipped.main_hand == Some(item_id) {
                        state.character.equipped.main_hand = None;
                    }
                    if state.character.equipped.off_hand == Some(item_id) {
                        state.character.equipped.off_hand = None;
                    }
                    if state.character.equipped.body == Some(item_id) {
                        state.character.equipped.body = None;
                    }
                    if let Some(item) = state.world.items.get_mut(&item_id) {
                        item.carried_by_player = false;
                        item.location = Some(current_location);
                    }
                    if let Some(loc) = state.world.locations.get_mut(&current_location) {
                        loc.items.push(item_id);
                    }
                    let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| item_name.clone());
                    vec![narration::templates::DROP_ITEM.replace("{item}", &name)]
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![format!("You don't have any \"{}\".", item_name)],
            }
        }
        Command::Use(item_name) => {
            let (lines, _consumed) = resolve_use_item(state, &mut rng, &item_name);
            lines
        }
        Command::Inventory => {
            if state.character.inventory.is_empty() {
                return vec![narration::templates::EMPTY_INVENTORY.to_string()];
            }
            let mut lines = vec!["You are carrying:".to_string()];
            for &item_id in &state.character.inventory {
                if let Some(item) = state.world.items.get(&item_id) {
                    let equipped_tag = if state.character.equipped.main_hand == Some(item_id) {
                        " (equipped - main hand)"
                    } else if state.character.equipped.off_hand == Some(item_id) {
                        " (equipped - off hand)"
                    } else if state.character.equipped.body == Some(item_id) {
                        " (equipped - body)"
                    } else {
                        ""
                    };
                    lines.push(format!("  - {}{}", item.name, equipped_tag));
                }
            }
            lines
        }
        Command::CharacterSheet => {
            narration::narrate_character_sheet(state)
        }
        Command::Check(skill_name) => {
            match parser::resolve_skill(&skill_name) {
                Some(skill) => {
                    let result = rules::checks::skill_check(
                        &mut rng, skill,
                        &state.character.ability_scores,
                        &state.character.skill_proficiencies,
                        state.character.proficiency_bonus(),
                        15, // Default DC for voluntary checks
                        false, false,
                    );
                    vec![narration::narrate_skill_check(&mut rng, &skill.to_string(), &result)]
                }
                None => vec![format!("Unknown skill: \"{}\". Try 'help' for a list.", skill_name)],
            }
        }
        Command::Save(name) => {
            let filename = name.unwrap_or_else(|| "autosave".to_string());
            vec![format!("[Game state ready to save as '{}.json'. Frontend handles file I/O.]", filename)]
        }
        Command::Load(name) => {
            let filename = name.unwrap_or_else(|| "autosave".to_string());
            vec![format!("[Load '{}.json'. Frontend handles file I/O.]", filename)]
        }
        Command::Help(topic) => {
            narration::templates::render_help(
                topic.as_deref(),
                narration::templates::HelpPhase::Exploration,
            )
        }
        Command::Objective => render_objective(state),
        Command::Map => render_map(state),
        Command::Spells => {
            spells::format_known_spells(
                &state.character.known_spells,
                &state.character.spell_slots_remaining,
                &state.character.spell_slots_max,
            )
        }
        Command::Equip(target_str) => {
            // Check for "off hand" suffix
            let words: Vec<&str> = target_str.split_whitespace().collect();
            let (target_name, force_off_hand) = if words.len() >= 3
                && words[words.len()-2] == "off" && words[words.len()-1] == "hand"
            {
                (words[..words.len()-2].join(" "), true)
            } else {
                (target_str.clone(), false)
            };

            if target_name.is_empty() {
                return vec!["Equip what?".to_string()];
            }

            let owned_candidates = inventory_item_candidates(state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&target_name, &candidates) {
                ResolveResult::Found(id) => {
                    let item_id = id as u32;
                    let item = match state.world.items.get(&item_id) {
                        Some(item) => item.clone(),
                        None => return vec![narration::templates::EQUIP_NOT_FOUND.replace("{name}", &target_name)],
                    };

                    match &item.item_type {
                        state::ItemType::Weapon { properties, .. } => {
                            let is_two_handed = properties & equipment::TWO_HANDED != 0;
                            let is_light = properties & equipment::LIGHT != 0;

                            // Reject non-LIGHT weapons for off-hand
                            if force_off_hand && !is_light {
                                return vec![format!("The {} is too unwieldy to wield in your off hand.", item.name)];
                            }
                            // Reject two-handed for off-hand
                            if force_off_hand && is_two_handed {
                                return vec![format!("The {} requires both hands.", item.name)];
                            }

                            let mut lines = Vec::new();

                            if is_two_handed {
                                // Auto-swap main hand first
                                if let Some(old_id) = state.character.equipped.main_hand.take() {
                                    if old_id != item_id {
                                        let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                        lines.push(narration::templates::EQUIP_SWAP_WEAPON
                                            .replace("{old}", &old_name)
                                            .replace("{new}", &item.name));
                                    }
                                }
                                // Auto-clear off hand for two-handed weapons
                                if let Some(oh_id) = state.character.equipped.off_hand.take() {
                                    let oh_name = state.world.items.get(&oh_id).map(|i| i.name.clone()).unwrap_or_default();
                                    lines.push(narration::templates::EQUIP_TWO_HAND_CLEAR
                                        .replace("{offhand}", &oh_name)
                                        .replace("{weapon}", &item.name));
                                }
                                state.character.equipped.main_hand = Some(item_id);
                                if lines.is_empty() {
                                    lines.push(narration::templates::EQUIP_WIELD.replace("{item}", &item.name));
                                }
                            } else if force_off_hand {
                                if let Some(old_id) = state.character.equipped.off_hand.replace(item_id) {
                                    if old_id != item_id {
                                        let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                        lines.push(narration::templates::EQUIP_SWAP_WEAPON
                                            .replace("{old}", &old_name)
                                            .replace("{new}", &item.name));
                                    }
                                }
                                if lines.is_empty() {
                                    lines.push(narration::templates::EQUIP_WIELD_OFF.replace("{item}", &item.name));
                                }
                            } else {
                                if let Some(old_id) = state.character.equipped.main_hand.replace(item_id) {
                                    if old_id != item_id {
                                        let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                        lines.push(narration::templates::EQUIP_SWAP_WEAPON
                                            .replace("{old}", &old_name)
                                            .replace("{new}", &item.name));
                                    }
                                }
                                if lines.is_empty() {
                                    lines.push(narration::templates::EQUIP_WIELD.replace("{item}", &item.name));
                                }
                            }

                            lines
                        }
                        state::ItemType::Armor { category, .. } => {
                            let mut lines = Vec::new();
                            if *category == state::ArmorCategory::Shield {
                                if let Some(old_id) = state.character.equipped.off_hand.replace(item_id) {
                                    if old_id != item_id {
                                        let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                        lines.push(narration::templates::EQUIP_SWAP_WEAPON
                                            .replace("{old}", &old_name)
                                            .replace("{new}", &item.name));
                                    }
                                }
                                if lines.is_empty() {
                                    lines.push(narration::templates::EQUIP_SHIELD.replace("{item}", &item.name));
                                }
                            } else {
                                if let Some(old_id) = state.character.equipped.body.replace(item_id) {
                                    if old_id != item_id {
                                        let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                        lines.push(narration::templates::EQUIP_SWAP_ARMOR
                                            .replace("{old}", &old_name)
                                            .replace("{new}", &item.name));
                                    }
                                }
                                if lines.is_empty() {
                                    lines.push(narration::templates::EQUIP_WEAR.replace("{item}", &item.name));
                                }
                            }
                            lines
                        }
                        _ => vec![narration::templates::EQUIP_CANT.replace("{item}", &item.name)],
                    }
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![narration::templates::EQUIP_NOT_FOUND.replace("{name}", &target_name)],
            }
        }
        Command::Unequip(target_str) => {
            // Build candidates from equipped items only
            let mut equipped_candidates: Vec<(usize, String)> = Vec::new();
            if let Some(mh) = state.character.equipped.main_hand {
                if let Some(item) = state.world.items.get(&mh) {
                    equipped_candidates.push((mh as usize, item.name.clone()));
                }
            }
            if let Some(oh) = state.character.equipped.off_hand {
                if let Some(item) = state.world.items.get(&oh) {
                    equipped_candidates.push((oh as usize, item.name.clone()));
                }
            }
            if let Some(body) = state.character.equipped.body {
                if let Some(item) = state.world.items.get(&body) {
                    equipped_candidates.push((body as usize, item.name.clone()));
                }
            }

            let candidates: Vec<(usize, &str)> = equipped_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&target_str, &candidates) {
                ResolveResult::Found(id) => {
                    let item_id = id as u32;
                    let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| target_str.clone());

                    let is_weapon = matches!(
                        state.world.items.get(&item_id).map(|i| &i.item_type),
                        Some(state::ItemType::Weapon { .. })
                    );

                    // Remove from whichever slot it's in
                    if state.character.equipped.main_hand == Some(item_id) {
                        state.character.equipped.main_hand = None;
                    }
                    if state.character.equipped.off_hand == Some(item_id) {
                        state.character.equipped.off_hand = None;
                    }
                    if state.character.equipped.body == Some(item_id) {
                        state.character.equipped.body = None;
                    }

                    if is_weapon {
                        vec![narration::templates::UNEQUIP_WEAPON.replace("{item}", &name)]
                    } else {
                        vec![narration::templates::UNEQUIP_ARMOR.replace("{item}", &name)]
                    }
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![narration::templates::UNEQUIP_NOT_EQUIPPED.replace("{name}", &target_str)],
            }
        }
        Command::Cast { spell, target: _ } => {
            // Check if caster
            if state.character.known_spells.is_empty() {
                return vec![narration::templates::CAST_NOT_A_CASTER.to_string()];
            }
            // Check if spell is known
            let spell_def = match spells::find_spell(&spell) {
                Some(def) if state.character.known_spells.iter().any(|s| s.to_lowercase() == def.name.to_lowercase()) => def,
                _ => return vec![narration::templates::CAST_UNKNOWN_SPELL.to_string()],
            };
            // In exploration, only Prestidigitation and Fire Bolt work
            match spell_def.name {
                "Prestidigitation" => {
                    vec![narration::templates::CAST_PRESTIDIGITATION.to_string()]
                }
                "Fire Bolt" => {
                    vec![narration::templates::CAST_FIRE_BOLT_EXPLORE.to_string()]
                }
                _ => {
                    vec![narration::templates::CAST_NOT_IN_COMBAT.to_string()]
                }
            }
        }
        Command::Attack(_) | Command::Approach(_) | Command::Retreat | Command::Dodge | Command::Disengage | Command::Dash | Command::EndTurn => {
            vec!["You're not in combat.".to_string()]
        }
        Command::NewGame => {
            vec!["You can only start a new game after being defeated.".to_string()]
        }
        Command::Unknown(s) => {
            if s.is_empty() {
                vec![]
            } else {
                vec![narration::templates::UNKNOWN_COMMAND.replace("{input}", &s)]
            }
        }
    }
}

fn process_npc_turns(state: &mut GameState, rng: &mut StdRng) -> Vec<String> {
    let mut lines = Vec::new();

    loop {
        // Take combat out to avoid borrow conflict
        let mut combat = match state.active_combat.take() {
            Some(c) => c,
            None => break,
        };

        if let Some(_) = combat.check_end(state) {
            state.active_combat = Some(combat);
            break;
        }

        if combat.is_player_turn() {
            state.active_combat = Some(combat);
            break;
        }

        let combatant = combat.current_combatant();
        if let combat::Combatant::Npc(npc_id) = combatant {
            let npc_lines = combat::resolve_npc_turn(rng, npc_id, state, &mut combat);
            lines.extend(npc_lines);
        }

        combat.advance_turn(state);
        state.active_combat = Some(combat);
    }

    lines
}

fn end_combat(state: &mut GameState, victory: bool) -> Vec<String> {
    state.active_combat = None;
    if victory {
        let mut lines = vec![
            String::new(),
            "=== VICTORY ===".to_string(),
            "All enemies have been defeated!".to_string(),
        ];
        if !state.progress.first_victory {
            state.progress.first_victory = true;
            lines.push("Objective complete: You survived your first battle.".to_string());
        }
        lines
    } else {
        vec![
            String::new(),
            "=== DEFEAT ===".to_string(),
            "You have fallen in battle...".to_string(),
            "Load a previous save or type `new game` to start over.".to_string(),
        ]
    }
}

fn append_player_turn_prompt(lines: &mut Vec<String>, state: &GameState, combat: &combat::CombatState) {
    lines.push(String::new());
    lines.push(format!(
        "Your turn! (Round {}, HP: {}/{})",
        combat.round,
        state.character.current_hp,
        state.character.max_hp
    ));
    let action_status = if combat.player_action_used { "used" } else { "available" };
    lines.push(format!(
        "Movement remaining: {} ft | Action: {}",
        combat.player_movement_remaining,
        action_status
    ));
    lines.extend(combat::format_enemy_summary(state, combat));
    lines.push("Commands: attack <target>, approach <target>, retreat, dodge, disengage, dash, end turn".to_string());
}

fn handle_combat(state: &mut GameState, input: &str) -> Vec<String> {
    let command = parser::parse(input);
    let mut rng = StdRng::seed_from_u64(state.rng_seed + state.rng_counter);
    state.rng_counter += 1;

    // Allow non-combat commands during combat (these don't consume combat state)
    match &command {
        Command::Look(target) => {
            if target.is_some() {
                return vec!["You're in combat! Use 'look' to see the battlefield.".to_string()];
            }
            let combat = state.active_combat.take().unwrap();
            let result = combat::format_combat_status(state, &combat);
            state.active_combat = Some(combat);
            return result;
        }
        Command::Inventory => {
            if state.character.inventory.is_empty() {
                return vec![narration::templates::EMPTY_INVENTORY.to_string()];
            }
            let mut inv_lines = vec!["You are carrying:".to_string()];
            for &item_id in &state.character.inventory {
                if let Some(item) = state.world.items.get(&item_id) {
                    let equipped_tag = if state.character.equipped.main_hand == Some(item_id) {
                        " (equipped - main hand)"
                    } else if state.character.equipped.off_hand == Some(item_id) {
                        " (equipped - off hand)"
                    } else if state.character.equipped.body == Some(item_id) {
                        " (equipped - body)"
                    } else {
                        ""
                    };
                    inv_lines.push(format!("  - {}{}", item.name, equipped_tag));
                }
            }
            return inv_lines;
        }
        Command::CharacterSheet => {
            return narration::narrate_character_sheet(state);
        }
        Command::Help(topic) => {
            return narration::templates::render_help(
                topic.as_deref(),
                narration::templates::HelpPhase::Combat,
            );
        }
        Command::Objective => return render_objective(state),
        Command::Map => return render_map(state),
        Command::Spells => {
            return spells::format_known_spells(
                &state.character.known_spells,
                &state.character.spell_slots_remaining,
                &state.character.spell_slots_max,
            );
        }
        // Block exploration commands
        Command::Go(_) | Command::Talk(_) | Command::Take(_) | Command::Drop(_) => {
            return vec!["You can't do that during combat!".to_string()];
        }
        Command::Save(_) | Command::Load(_) | Command::Check(_) => {
            return vec!["You can't do that during combat!".to_string()];
        }
        _ => {}
    }

    // Take combat out for the duration of action processing
    let mut combat = state.active_combat.take().unwrap();

    if !combat.is_player_turn() {
        state.active_combat = Some(combat);
        return vec!["It's not your turn!".to_string()];
    }

    let mut lines = Vec::new();
    let mut should_end_turn = false;

    match command {
        Command::Attack(target_name) => {
            let owned_candidates = {
                // Build candidates from combat initiative order
                combat.initiative_order.iter()
                    .filter_map(|(c, _)| {
                        if let combat::Combatant::Npc(id) = c {
                            let npc = state.world.npcs.get(id)?;
                            let stats = npc.combat_stats.as_ref()?;
                            if stats.current_hp > 0 {
                                Some((*id as usize, npc.name.clone()))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            };
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&target_name, &candidates) {
                ResolveResult::Found(id) => {
                    let npc_id = id as u32;

                    if combat.player_action_used {
                        state.active_combat = Some(combat);
                        return vec!["You've already used your action this turn. You can still move (approach/retreat).".to_string()];
                    }

                    let distance = *combat.distances.get(&npc_id).unwrap_or(&30);
                    let target_ac = state.world.npcs.get(&npc_id)
                        .and_then(|n| n.combat_stats.as_ref())
                        .map(|s| s.ac)
                        .unwrap_or(10);
                    let target_dodging = combat.npc_dodging.get(&npc_id).copied().unwrap_or(false);

                    let weapon_id = state.character.equipped.main_hand;
                    let off_hand_free = state.character.equipped.off_hand.is_none();

                    // Check range
                    if let Some(weapon_item) = weapon_id.and_then(|id| state.world.items.get(&id)) {
                        if let state::ItemType::Weapon { range_normal, range_long, properties, .. } = &weapon_item.item_type {
                            let is_reach = properties & crate::equipment::REACH != 0;
                            let melee_range = if is_reach { 10 } else { 5 };
                            let is_melee_only = *range_normal == 0 && *range_long == 0;

                            if is_melee_only && distance > melee_range {
                                let msg = format!("You're too far away to attack with {}. Move closer first (approach <target>).",
                                    weapon_item.name);
                                state.active_combat = Some(combat);
                                return vec![msg];
                            }
                            if *range_long > 0 && distance > *range_long as u32 {
                                let msg = format!("The target is out of range of your {}.", weapon_item.name);
                                state.active_combat = Some(combat);
                                return vec![msg];
                            }
                        }
                    } else if weapon_id.is_none() && distance > 5 {
                        state.active_combat = Some(combat);
                        return vec!["You're too far away for an unarmed strike. Move closer first (approach <target>).".to_string()];
                    }

                    let hostile_within_5ft = combat::has_living_hostile_within(state, &combat, 5);

                    // Get target conditions for attack resolution
                    let target_conditions: &[crate::conditions::ActiveCondition] = state.world.npcs.get(&npc_id)
                        .map(|n| n.conditions.as_slice())
                        .unwrap_or(&[]);

                    let result = combat::resolve_player_attack(
                        &mut rng, &state.character, target_ac, target_dodging,
                        weapon_id, &state.world.items, distance, off_hand_free, hostile_within_5ft,
                        target_conditions,
                    );

                    let npc_name = state.world.npcs.get(&npc_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "the enemy".to_string());

                    if result.weapon_name == "Unarmed" {
                        lines.push(format!("You punch {} for {} {} damage.",
                            npc_name, result.damage, result.damage_type));
                    } else if result.hit {
                        if result.natural_20 {
                            lines.push(format!("You attack {} with {} -- CRITICAL HIT! {} {} damage!",
                                npc_name, result.weapon_name, result.damage, result.damage_type));
                        } else {
                            lines.push(format!("You attack {} with {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                                npc_name, result.weapon_name, result.attack_roll,
                                result.total_attack - result.attack_roll, result.total_attack, target_ac,
                                result.damage, result.damage_type));
                        }
                    } else if result.natural_1 {
                        lines.push(format!("You attack {} with {} -- natural 1, miss!",
                            npc_name, result.weapon_name));
                    } else {
                        lines.push(format!("You attack {} with {} ({}+{}={} vs AC {}) -- miss.",
                            npc_name, result.weapon_name, result.attack_roll,
                            result.total_attack - result.attack_roll, result.total_attack, target_ac));
                    }
                    if result.disadvantage {
                        lines.push("(Rolled with disadvantage)".to_string());
                    }

                    // Apply damage
                    if result.hit {
                        if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                            if let Some(stats) = npc.combat_stats.as_mut() {
                                stats.current_hp -= result.damage;
                                if stats.current_hp <= 0 {
                                    stats.current_hp = 0;
                                    lines.push(format!("{} is slain!", npc_name));
                                }
                            }
                        }
                    }

                    combat.player_action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                ResolveResult::Ambiguous(matches) => {
                    state.active_combat = Some(combat);
                    return resolver::format_disambiguation(&matches);
                }
                ResolveResult::NotFound => {
                    state.active_combat = Some(combat);
                    return vec![format!("There's no \"{}\" to attack.", target_name)];
                }
            }
        }
        Command::Approach(target_name) => {
            let owned_candidates: Vec<(usize, String)> = combat.initiative_order.iter()
                .filter_map(|(c, _)| {
                    if let combat::Combatant::Npc(id) = c {
                        let npc = state.world.npcs.get(id)?;
                        let stats = npc.combat_stats.as_ref()?;
                        if stats.current_hp > 0 { Some((*id as usize, npc.name.clone())) } else { None }
                    } else { None }
                })
                .collect();
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&target_name, &candidates) {
                ResolveResult::Found(id) => {
                    let npc_id = id as u32;
                    let approach_lines = combat::approach_target(&mut rng, npc_id, state, &mut combat);
                    lines.extend(approach_lines);
                }
                ResolveResult::Ambiguous(matches) => {
                    state.active_combat = Some(combat);
                    return resolver::format_disambiguation(&matches);
                }
                ResolveResult::NotFound => {
                    state.active_combat = Some(combat);
                    return vec![format!("There's no \"{}\" here.", target_name)];
                }
            }
        }
        Command::Retreat => {
            let retreat_lines = combat::retreat(&mut rng, state, &mut combat);
            lines.extend(retreat_lines);
        }
        Command::Dodge => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_dodging = true;
            combat.player_action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
            lines.push("You take the Dodge action. Attacks against you have disadvantage until your next turn.".to_string());
        }
        Command::Disengage => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_disengaging = true;
            combat.player_action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
            lines.push("You take the Disengage action. You can retreat without provoking opportunity attacks.".to_string());
        }
        Command::Dash => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_movement_remaining += state.character.speed;
            combat.player_action_used = true;
            should_end_turn = false;
            lines.push(format!("You take the Dash action. Movement this turn: {} ft.", combat.player_movement_remaining));
        }
        Command::Equip(target_str) => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            lines.extend(handle_equip_command(state, &target_str));
            combat.player_action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::Unequip(target_str) => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            lines.extend(handle_unequip_command(state, &target_str));
            combat.player_action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::Use(item_name) => {
            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn. You can still move (approach/retreat).".to_string()];
            }
            let (mut use_lines, consumed_action) = resolve_use_item(state, &mut rng, &item_name);
            lines.append(&mut use_lines);
            if consumed_action {
                combat.player_action_used = true;
            }
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::EndTurn => {
            lines.push("You end your turn.".to_string());
            should_end_turn = true;
        }
        Command::Cast { spell, target } => {
            // Check if caster
            if state.character.known_spells.is_empty() {
                state.active_combat = Some(combat);
                return vec![narration::templates::CAST_NOT_A_CASTER.to_string()];
            }
            // Check if spell is known
            let spell_def = match spells::find_spell(&spell) {
                Some(def) if state.character.known_spells.iter().any(|s| s.to_lowercase() == def.name.to_lowercase()) => def,
                _ => {
                    state.active_combat = Some(combat);
                    return vec![narration::templates::CAST_UNKNOWN_SPELL.to_string()];
                }
            };

            if combat.player_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }

            // Check spell slots
            if !spells::consume_spell_slot(spell_def.level, &mut state.character.spell_slots_remaining) {
                state.active_combat = Some(combat);
                return vec![narration::templates::CAST_NO_SLOTS.to_string()];
            }

            let int_score = state.character.ability_scores.get(&Ability::Intelligence).copied().unwrap_or(10);
            let prof_bonus = state.character.proficiency_bonus();

            match spell_def.name {
                "Fire Bolt" => {
                    // Needs a target
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            // Undo slot consumption (cantrip so no slot was consumed anyway)
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET.replace("{spell}", "Fire Bolt")];
                        }
                    };

                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str()))
                        .collect();

                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let target_ac = state.world.npcs.get(&npc_id)
                                .and_then(|n| n.combat_stats.as_ref())
                                .map(|s| s.ac)
                                .unwrap_or(10);
                            let npc_name = state.world.npcs.get(&npc_id)
                                .map(|n| n.name.clone())
                                .unwrap_or_else(|| "the enemy".to_string());

                            let outcome = spells::resolve_fire_bolt(&mut rng, int_score, prof_bonus, target_ac);
                            if let spells::CastOutcome::FireBolt { attack, damage } = outcome {
                                if attack.hit {
                                    if attack.natural_20 {
                                        lines.push(narration::templates::CAST_FIRE_BOLT_CRIT
                                            .replace("{target}", &npc_name)
                                            .replace("{damage}", &damage.to_string()));
                                    } else {
                                        lines.push(narration::templates::CAST_FIRE_BOLT_HIT
                                            .replace("{target}", &npc_name)
                                            .replace("{roll}", &attack.roll.to_string())
                                            .replace("{mod}", &attack.modifier.to_string())
                                            .replace("{total}", &attack.total.to_string())
                                            .replace("{ac}", &target_ac.to_string())
                                            .replace("{damage}", &damage.to_string()));
                                    }
                                    // Apply damage
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        if let Some(stats) = npc.combat_stats.as_mut() {
                                            stats.current_hp -= damage;
                                            if stats.current_hp <= 0 {
                                                stats.current_hp = 0;
                                                lines.push(format!("{} is slain!", npc_name));
                                            }
                                        }
                                    }
                                } else if attack.natural_1 {
                                    lines.push(narration::templates::CAST_FIRE_BOLT_MISS_NAT1
                                        .replace("{target}", &npc_name));
                                } else {
                                    lines.push(narration::templates::CAST_FIRE_BOLT_MISS
                                        .replace("{target}", &npc_name)
                                        .replace("{roll}", &attack.roll.to_string())
                                        .replace("{mod}", &attack.modifier.to_string())
                                        .replace("{total}", &attack.total.to_string())
                                        .replace("{ac}", &target_ac.to_string()));
                                }
                            }
                            combat.player_action_used = true;
                            should_end_turn = combat.player_movement_remaining <= 0;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.active_combat = Some(combat);
                            return resolver::format_disambiguation(&matches);
                        }
                        ResolveResult::NotFound => {
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                "Prestidigitation" => {
                    lines.push(narration::templates::CAST_PRESTIDIGITATION.to_string());
                    combat.player_action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                "Magic Missile" => {
                    // Needs a target
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            // Undo slot consumption
                            state.character.spell_slots_remaining[0] += 1;
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET.replace("{spell}", "Magic Missile")];
                        }
                    };

                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str()))
                        .collect();

                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let npc_name = state.world.npcs.get(&npc_id)
                                .map(|n| n.name.clone())
                                .unwrap_or_else(|| "the enemy".to_string());

                            let outcome = spells::resolve_magic_missile(&mut rng);
                            if let spells::CastOutcome::MagicMissile { darts, total_damage } = outcome {
                                lines.push(narration::templates::CAST_MAGIC_MISSILE
                                    .replace("{target}", &npc_name)
                                    .replace("{d1}", &darts[0].to_string())
                                    .replace("{d2}", &darts[1].to_string())
                                    .replace("{d3}", &darts[2].to_string())
                                    .replace("{total}", &total_damage.to_string()));

                                // Slot usage message
                                let remaining = state.character.spell_slots_remaining[0];
                                let max = state.character.spell_slots_max[0];
                                lines.push(narration::templates::CAST_SLOT_USED
                                    .replace("{remaining}", &remaining.to_string())
                                    .replace("{max}", &max.to_string())
                                    .replace("{level}", "1"));

                                // Apply damage
                                if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                    if let Some(stats) = npc.combat_stats.as_mut() {
                                        stats.current_hp -= total_damage;
                                        if stats.current_hp <= 0 {
                                            stats.current_hp = 0;
                                            lines.push(format!("{} is slain!", npc_name));
                                        }
                                    }
                                }
                            }
                            combat.player_action_used = true;
                            should_end_turn = combat.player_movement_remaining <= 0;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return resolver::format_disambiguation(&matches);
                        }
                        ResolveResult::NotFound => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                "Burning Hands" => {
                    // AoE -- hits all enemies within 5 ft, no target needed
                    let targets: Vec<spells::SpellTarget> = build_spell_targets(&combat, state);

                    let melee_count = targets.iter().filter(|t| t.distance <= 5).count();
                    if melee_count == 0 {
                        state.character.spell_slots_remaining[0] += 1; // refund
                        lines.push(narration::templates::CAST_BURNING_HANDS_NO_TARGETS.to_string());
                        state.active_combat = Some(combat);
                        return lines;
                    }

                    let outcome = spells::resolve_burning_hands(&mut rng, int_score, prof_bonus, &targets);
                    if let spells::CastOutcome::BurningHands { total_rolled, half_damage: _, dc, results } = outcome {
                        lines.push(narration::templates::CAST_BURNING_HANDS_INTRO
                            .replace("{damage}", &total_rolled.to_string())
                            .replace("{dc}", &dc.to_string()));

                        for result in &results {
                            let save_str = format!("{}+{}={} vs DC {}",
                                result.save_result.roll, result.save_result.modifier,
                                result.save_result.total, result.save_result.dc);
                            if result.save_result.saved {
                                lines.push(narration::templates::CAST_BURNING_HANDS_SAVE
                                    .replace("{target}", &result.name)
                                    .replace("{save_result}", &save_str)
                                    .replace("{damage}", &result.damage_taken.to_string()));
                            } else {
                                lines.push(narration::templates::CAST_BURNING_HANDS_FAIL
                                    .replace("{target}", &result.name)
                                    .replace("{save_result}", &save_str)
                                    .replace("{damage}", &result.damage_taken.to_string()));
                            }
                        }

                        // Slot usage message
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        lines.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));

                        // Apply damage to NPCs
                        for result in &results {
                            // Find NPC by name
                            for (_, npc) in state.world.npcs.iter_mut() {
                                if npc.name == result.name {
                                    if let Some(stats) = npc.combat_stats.as_mut() {
                                        stats.current_hp -= result.damage_taken;
                                        if stats.current_hp <= 0 {
                                            stats.current_hp = 0;
                                            lines.push(format!("{} is slain!", result.name));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    combat.player_action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                "Sleep" => {
                    // AoE by HP pool, targets weakest first
                    let targets: Vec<spells::SpellTarget> = build_spell_targets(&combat, state);

                    let outcome = spells::resolve_sleep(&mut rng, &targets);
                    if let spells::CastOutcome::SleepResult { hp_pool, affected } = outcome {
                        lines.push(narration::templates::CAST_SLEEP_INTRO
                            .replace("{pool}", &hp_pool.to_string()));

                        if affected.is_empty() {
                            lines.push(narration::templates::CAST_SLEEP_NONE.to_string());
                        } else {
                            for target in &affected {
                                lines.push(narration::templates::CAST_SLEEP_TARGET
                                    .replace("{target}", &target.name)
                                    .replace("{hp}", &target.hp.to_string()));

                                // Set HP to 0 (treated as defeated for combat-end purposes)
                                for (_, npc) in state.world.npcs.iter_mut() {
                                    if npc.name == target.name {
                                        if let Some(stats) = npc.combat_stats.as_mut() {
                                            stats.current_hp = 0;
                                        }
                                    }
                                }
                            }
                        }

                        // Slot usage message
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        lines.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));
                    }
                    combat.player_action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                "Shield" => {
                    let outcome = spells::resolve_shield();
                    if let spells::CastOutcome::ShieldCast { ac_bonus: _ } = outcome {
                        lines.push(narration::templates::CAST_SHIELD.to_string());

                        // Slot usage message
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        lines.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));

                        // Track shield AC bonus in combat state
                        // For MVP, we store shield bonus in a simple field
                        combat.player_shield_ac_bonus = 5;
                    }
                    combat.player_action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                _ => {
                    lines.push("That spell is not implemented yet.".to_string());
                }
            }
        }
        Command::Unknown(s) => {
            state.active_combat = Some(combat);
            if s.is_empty() {
                return vec![];
            }
            return vec![format!("Unknown combat command: \"{}\". Type 'help' for commands.", s)];
        }
        _ => {
            state.active_combat = Some(combat);
            return vec!["You can't do that during combat!".to_string()];
        }
    }

    // After player command, check if combat ended
    if let Some(victory) = combat.check_end(state) {
        state.active_combat = Some(combat);
        lines.extend(end_combat(state, victory));
        return lines;
    }

    // Keep player's turn open unless explicitly ending turn (or no meaningful options remain)
    if !should_end_turn {
        append_player_turn_prompt(&mut lines, state, &combat);
        state.active_combat = Some(combat);
        return lines;
    }

    // End player's turn: advance and process NPC turns
    combat.advance_turn(state);
    state.active_combat = Some(combat);

    let npc_lines = process_npc_turns(state, &mut rng);
    lines.extend(npc_lines);

    // Check combat end again after NPC turns
    if let Some(ref combat) = state.active_combat {
        if let Some(victory) = combat.check_end(state) {
            lines.extend(end_combat(state, victory));
            return lines;
        }
        append_player_turn_prompt(&mut lines, state, combat);
    }

    lines
}

fn resolve_use_item(
    state: &mut GameState,
    rng: &mut StdRng,
    item_name: &str,
) -> (Vec<String>, bool) {
    let owned_candidates = inventory_item_candidates(state);
    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();

    match resolver::resolve_target(&item_name, &candidates) {
        ResolveResult::Found(id) => {
            let item_id = id as u32;
            let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| item_name.to_string());
            let item_type = state.world.items.get(&item_id).map(|i| i.item_type.clone());
            match item_type {
                Some(state::ItemType::Consumable { ref effect }) => {
                    let result = match effect.as_str() {
                        "heal_1d8" => {
                            let rolls = rules::dice::roll_dice(rng, 1, 8);
                            let roll_total: i32 = rolls.iter().sum();
                            let old_hp = state.character.current_hp;
                            state.character.current_hp = (state.character.current_hp + roll_total).min(state.character.max_hp);
                            let healed = state.character.current_hp - old_hp;
                            if healed > 0 {
                                vec![narration::templates::USE_HEAL
                                    .replace("{item}", &name)
                                    .replace("{roll}", &healed.to_string())
                                    .replace("{current}", &state.character.current_hp.to_string())
                                    .replace("{max}", &state.character.max_hp.to_string())]
                            } else {
                                vec![narration::templates::USE_HEAL_FULL
                                    .replace("{item}", &name)
                                    .replace("{current}", &state.character.current_hp.to_string())
                                    .replace("{max}", &state.character.max_hp.to_string())]
                            }
                        }
                        "light" => {
                            let loc_id = state.current_location;
                            if let Some(loc) = state.world.locations.get_mut(&loc_id) {
                                match loc.light_level {
                                    state::LightLevel::Dark => {
                                        loc.light_level = state::LightLevel::Dim;
                                        vec![narration::templates::USE_LIGHT_UPGRADE
                                            .replace("{item}", &name)
                                            .replace("{old_level}", "dark")
                                            .replace("{new_level}", "dim")]
                                    }
                                    state::LightLevel::Dim => {
                                        loc.light_level = state::LightLevel::Bright;
                                        vec![narration::templates::USE_LIGHT_UPGRADE
                                            .replace("{item}", &name)
                                            .replace("{old_level}", "dim")
                                            .replace("{new_level}", "bright")]
                                    }
                                    state::LightLevel::Bright => {
                                        vec![narration::templates::USE_LIGHT_ALREADY_BRIGHT
                                            .replace("{item}", &name)]
                                    }
                                }
                            } else {
                                vec![narration::templates::USE_UNKNOWN_EFFECT.replace("{item}", &name)]
                            }
                        }
                        "nourish" => {
                            vec![narration::templates::USE_NOURISH.replace("{item}", &name)]
                        }
                        _ => {
                            vec![narration::templates::USE_UNKNOWN_EFFECT.replace("{item}", &name)]
                        }
                    };
                    // Consume the item: remove from inventory and world
                    state.character.inventory.retain(|&id| id != item_id);
                    state.world.items.remove(&item_id);
                    (result, true) // action consumed
                }
                _ => {
                    (vec![narration::templates::USE_NOT_CONSUMABLE.replace("{item}", &name)], false)
                }
            }
        }
        ResolveResult::Ambiguous(matches) => (resolver::format_disambiguation(&matches), false),
        ResolveResult::NotFound => (vec![format!("You don't have any \"{}\".", item_name)], false),
    }
}

fn render_objective(state: &GameState) -> Vec<String> {
    if state.progress.first_victory {
        vec![
            "Objective: Keep exploring the ruins for deeper threats and treasure.".to_string(),
            "Progress: First battle survived.".to_string(),
        ]
    } else {
        vec![
            "Objective: Defeat a hostile foe.".to_string(),
            "Tip: Enter rooms with hostile NPCs to trigger combat.".to_string(),
        ]
    }
}

fn render_map(state: &GameState) -> Vec<String> {
    let mut ids: Vec<_> = state.discovered_locations.iter().copied().collect();
    ids.sort_unstable();

    if ids.is_empty() {
        return vec!["=== MAP ===".to_string(), "No locations discovered yet.".to_string()];
    }

    let mut lines = vec!["=== MAP ===".to_string()];
    for id in ids {
        if let Some(loc) = state.world.locations.get(&id) {
            let marker = if id == state.current_location { "*" } else { " " };
            let mut exits: Vec<String> = loc.exits.keys().map(|d| d.to_string()).collect();
            exits.sort();
            lines.push(format!("{} {} [{}] -> {}", marker, loc.name, id, exits.join(", ")));
        }
    }
    lines
}

fn handle_equip_command(state: &mut GameState, target_str: &str) -> Vec<String> {
    let words: Vec<&str> = target_str.split_whitespace().collect();
    let (target_name, force_off_hand) = if words.len() >= 3
        && words[words.len()-2] == "off" && words[words.len()-1] == "hand"
    {
        (words[..words.len()-2].join(" "), true)
    } else {
        (target_str.to_string(), false)
    };

    if target_name.is_empty() {
        return vec!["Equip what?".to_string()];
    }

    let owned_candidates = inventory_item_candidates(state);
    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();

    match resolver::resolve_target(&target_name, &candidates) {
        ResolveResult::Found(id) => {
            let item_id = id as u32;
            let item = match state.world.items.get(&item_id) {
                Some(item) => item.clone(),
                None => return vec![narration::templates::EQUIP_NOT_FOUND.replace("{name}", &target_name)],
            };

            match &item.item_type {
                state::ItemType::Weapon { properties, .. } => {
                    let is_two_handed = properties & equipment::TWO_HANDED != 0;
                    let is_light = properties & equipment::LIGHT != 0;
                    if force_off_hand && !is_light {
                        return vec![format!("The {} is too unwieldy to wield in your off hand.", item.name)];
                    }
                    if force_off_hand && is_two_handed {
                        return vec![format!("The {} requires both hands.", item.name)];
                    }
                    let mut result_lines = Vec::new();
                    if is_two_handed {
                        if let Some(old_id) = state.character.equipped.main_hand.take() {
                            if old_id != item_id {
                                let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                result_lines.push(narration::templates::EQUIP_SWAP_WEAPON.replace("{old}", &old_name).replace("{new}", &item.name));
                            }
                        }
                        if let Some(oh_id) = state.character.equipped.off_hand.take() {
                            let oh_name = state.world.items.get(&oh_id).map(|i| i.name.clone()).unwrap_or_default();
                            result_lines.push(narration::templates::EQUIP_TWO_HAND_CLEAR.replace("{offhand}", &oh_name).replace("{weapon}", &item.name));
                        }
                        state.character.equipped.main_hand = Some(item_id);
                        if result_lines.is_empty() {
                            result_lines.push(narration::templates::EQUIP_WIELD.replace("{item}", &item.name));
                        }
                    } else if force_off_hand {
                        if let Some(old_id) = state.character.equipped.off_hand.replace(item_id) {
                            if old_id != item_id {
                                let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                result_lines.push(narration::templates::EQUIP_SWAP_WEAPON.replace("{old}", &old_name).replace("{new}", &item.name));
                            }
                        }
                        if result_lines.is_empty() {
                            result_lines.push(narration::templates::EQUIP_WIELD_OFF.replace("{item}", &item.name));
                        }
                    } else {
                        if let Some(old_id) = state.character.equipped.main_hand.replace(item_id) {
                            if old_id != item_id {
                                let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                result_lines.push(narration::templates::EQUIP_SWAP_WEAPON.replace("{old}", &old_name).replace("{new}", &item.name));
                            }
                        }
                        if result_lines.is_empty() {
                            result_lines.push(narration::templates::EQUIP_WIELD.replace("{item}", &item.name));
                        }
                    }
                    result_lines
                }
                state::ItemType::Armor { category, .. } => {
                    let mut result_lines = Vec::new();
                    if *category == state::ArmorCategory::Shield {
                        if let Some(old_id) = state.character.equipped.off_hand.replace(item_id) {
                            if old_id != item_id {
                                let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                result_lines.push(narration::templates::EQUIP_SWAP_WEAPON.replace("{old}", &old_name).replace("{new}", &item.name));
                            }
                        }
                        if result_lines.is_empty() {
                            result_lines.push(narration::templates::EQUIP_SHIELD.replace("{item}", &item.name));
                        }
                    } else {
                        if let Some(old_id) = state.character.equipped.body.replace(item_id) {
                            if old_id != item_id {
                                let old_name = state.world.items.get(&old_id).map(|i| i.name.clone()).unwrap_or_default();
                                result_lines.push(narration::templates::EQUIP_SWAP_ARMOR.replace("{old}", &old_name).replace("{new}", &item.name));
                            }
                        }
                        if result_lines.is_empty() {
                            result_lines.push(narration::templates::EQUIP_WEAR.replace("{item}", &item.name));
                        }
                    }
                    result_lines
                }
                _ => vec![narration::templates::EQUIP_CANT.replace("{item}", &item.name)],
            }
        }
        ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
        ResolveResult::NotFound => vec![narration::templates::EQUIP_NOT_FOUND.replace("{name}", &target_name)],
    }
}

fn handle_unequip_command(state: &mut GameState, target_str: &str) -> Vec<String> {
    let mut equipped_candidates: Vec<(usize, String)> = Vec::new();
    if let Some(mh) = state.character.equipped.main_hand {
        if let Some(item) = state.world.items.get(&mh) {
            equipped_candidates.push((mh as usize, item.name.clone()));
        }
    }
    if let Some(oh) = state.character.equipped.off_hand {
        if let Some(item) = state.world.items.get(&oh) {
            equipped_candidates.push((oh as usize, item.name.clone()));
        }
    }
    if let Some(body) = state.character.equipped.body {
        if let Some(item) = state.world.items.get(&body) {
            equipped_candidates.push((body as usize, item.name.clone()));
        }
    }

    let candidates: Vec<(usize, &str)> = equipped_candidates.iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();

    match resolver::resolve_target(target_str, &candidates) {
        ResolveResult::Found(id) => {
            let item_id = id as u32;
            let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| target_str.to_string());
            let is_weapon = matches!(
                state.world.items.get(&item_id).map(|i| &i.item_type),
                Some(state::ItemType::Weapon { .. })
            );
            if state.character.equipped.main_hand == Some(item_id) { state.character.equipped.main_hand = None; }
            if state.character.equipped.off_hand == Some(item_id) { state.character.equipped.off_hand = None; }
            if state.character.equipped.body == Some(item_id) { state.character.equipped.body = None; }
            if is_weapon {
                vec![narration::templates::UNEQUIP_WEAPON.replace("{item}", &name)]
            } else {
                vec![narration::templates::UNEQUIP_ARMOR.replace("{item}", &name)]
            }
        }
        ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
        ResolveResult::NotFound => vec![narration::templates::UNEQUIP_NOT_EQUIPPED.replace("{name}", target_str)],
    }
}

// Helper methods on NPC for narration
impl state::Npc {
    pub fn role_description(&self) -> &'static str {
        match self.role {
            state::NpcRole::Merchant => "a merchant",
            state::NpcRole::Guard => "a guard",
            state::NpcRole::Hermit => "a hermit",
            state::NpcRole::Adventurer => "an adventurer",
        }
    }

    pub fn disposition_description(&self) -> &'static str {
        match self.disposition {
            state::Disposition::Friendly => "seems friendly",
            state::Disposition::Neutral => "regards you neutrally",
            state::Disposition::Hostile => "eyes you with hostility",
        }
    }

    pub fn generate_dialogue(&self, rng: &mut impl rand::Rng) -> Vec<String> {
        let greetings = match self.disposition {
            state::Disposition::Friendly => &["\"Well met, traveler!\"", "\"Welcome, friend!\"", "\"Good to see a friendly face.\""][..],
            state::Disposition::Neutral => &["\"What do you want?\"", "\"Hmm?\"", "\"State your business.\""][..],
            state::Disposition::Hostile => &["\"Get out of my sight.\"", "\"You don't belong here.\"", "\"Leave. Now.\""][..],
        };
        let greeting = greetings[rng.gen_range(0..greetings.len())];

        let flavor = match self.role {
            state::NpcRole::Merchant => "\"I have wares, if you have coin.\"",
            state::NpcRole::Guard => "\"Keep your hands where I can see them.\"",
            state::NpcRole::Hermit => "\"The walls whisper secrets to those who listen...\"",
            state::NpcRole::Adventurer => "\"I've heard rumors of treasure deeper within.\"",
        };

        vec![
            format!("{} says:", self.name),
            greeting.to_string(),
            flavor.to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_game_returns_creation_prompt() {
        let output = new_game(42, false);
        assert!(output.text.iter().any(|t| t.contains("Choose your race")));
    }

    #[test]
    fn test_new_game_sets_ironman_mode_false_when_disabled() {
        let output = new_game(42, false);
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!state.ironman_mode);
    }

    #[test]
    fn test_new_game_sets_ironman_mode_true_when_enabled() {
        let output = new_game(42, true);
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(state.ironman_mode);
    }

    #[test]
    fn test_full_character_creation_flow() {
        let output = new_game(42, false);
        let state = &output.state_json;

        // Choose race
        let output = process_input(state, "1");
        assert!(output.text.iter().any(|t| t.contains("class")));

        // Choose class
        let output = process_input(&output.state_json, "1");
        assert!(output.text.iter().any(|t| t.contains("ability score")));

        // Choose standard array
        let output = process_input(&output.state_json, "1");
        assert!(output.text.iter().any(|t| t.contains("STR DEX CON")));

        // Assign scores
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        assert!(output.text.iter().any(|t| t.contains("skill")));

        // Choose skills (Fighter gets 2)
        let output = process_input(&output.state_json, "1 2");
        assert!(output.text.iter().any(|t| t.contains("name")));

        // Choose name
        let output = process_input(&output.state_json, "Aldric");
        assert!(output.text.iter().any(|t| t.contains("Aldric")));

        // Verify we're in exploration
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::Exploration));
        assert!(!state.world.locations.is_empty());
    }

    #[test]
    fn test_look_command() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "look");
        assert!(!output.text.is_empty());
    }

    #[test]
    fn test_invalid_direction() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        // Try a direction that may not exist
        let output = process_input(&state_json, "go up");
        // Should either move or say can't go
        assert!(!output.text.is_empty());
    }

    #[test]
    fn test_help_command() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "help");
        assert!(output.text.iter().any(|t| t.contains("Commands")));
    }

    #[test]
    fn test_inventory_empty() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "inventory");
        assert!(output.text.iter().any(|t| t.contains("carrying") || t.contains("anything")));
    }

    #[test]
    fn test_look_at_inventory_item() {
        let mut state = create_test_exploration_state();
        // Give the player an item
        let item_id = *state.world.items.keys().next().unwrap();
        let item_name = state.world.items.get(&item_id).unwrap().name.clone();
        state.world.items.get_mut(&item_id).unwrap().carried_by_player = true;
        state.character.inventory.push(item_id);
        // Remove from location so it's only in inventory
        for loc in state.world.locations.values_mut() {
            loc.items.retain(|&id| id != item_id);
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, &format!("look {}", item_name.to_lowercase()));
        assert!(output.text.iter().any(|t| t.contains(&item_name)), "Should find item '{}' in inventory. Got: {:?}", item_name, output.text);
    }

    #[test]
    fn test_drop_command() {
        let mut state = create_test_exploration_state();
        // Give the player an item
        let item_id = *state.world.items.keys().next().unwrap();
        let item_name = state.world.items.get(&item_id).unwrap().name.clone();
        state.world.items.get_mut(&item_id).unwrap().carried_by_player = true;
        state.character.inventory.push(item_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, &format!("drop {}", item_name.to_lowercase()));
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("drop")), "Should confirm drop. Got: {:?}", output.text);

        // Verify item is no longer in inventory
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.inventory.contains(&item_id));
    }

    #[test]
    fn test_fuzzy_target_npc() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let loc = state.world.locations.get(&state.current_location).unwrap();
        if let Some(&npc_id) = loc.npcs.first() {
            let npc = state.world.npcs.get(&npc_id).unwrap();
            let first_3_chars: String = npc.name.chars().take(3).collect();
            let output = process_input(&state_json, &format!("talk {}", first_3_chars.to_lowercase()));
            let all_text = output.text.join(" ");
            assert!(!all_text.contains("no one called"), "Fuzzy match should find NPC with prefix '{}'. Got: {:?}", first_3_chars, output.text);
        }
    }

    #[test]
    fn test_talk_to_verb_phrase() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let loc = state.world.locations.get(&state.current_location).unwrap();
        if let Some(&npc_id) = loc.npcs.first() {
            let npc = state.world.npcs.get(&npc_id).unwrap();
            let name_lower = npc.name.to_lowercase();
            let output = process_input(&state_json, &format!("talk to {}", name_lower));
            assert!(output.text.iter().any(|t| t.contains("says:")), "Should talk to NPC. Got: {:?}", output.text);
        }
    }

    #[test]
    fn test_equip_weapon_from_inventory() {
        let mut state = create_test_exploration_state();
        // Find a weapon in the world
        let weapon_id = state.world.items.iter()
            .find(|(_, item)| matches!(item.item_type, state::ItemType::Weapon { .. }))
            .map(|(&id, _)| id).unwrap();
        let weapon_name = state.world.items[&weapon_id].name.clone();
        // Put it in inventory
        state.world.items.get_mut(&weapon_id).unwrap().carried_by_player = true;
        state.character.inventory.push(weapon_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, &format!("equip {}", weapon_name.to_lowercase()));
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.character.equipped.main_hand == Some(weapon_id),
            "Weapon should be in main hand. Got: {:?}", new_state.character.equipped);
    }

    #[test]
    fn test_equip_armor_from_inventory() {
        let mut state = create_test_exploration_state();
        // Find or create an armor item
        let armor_id = state.world.items.keys().max().unwrap() + 1;
        state.world.items.insert(armor_id, state::Item {
            id: armor_id, name: "Leather".to_string(), description: "Leather armor.".to_string(),
            item_type: state::ItemType::Armor {
                category: state::ArmorCategory::Light, base_ac: 11,
                max_dex_bonus: None, str_requirement: 0, stealth_disadvantage: false,
            },
            location: None, carried_by_player: true,
        });
        state.character.inventory.push(armor_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "equip leather");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.equipped.body, Some(armor_id));
    }

    #[test]
    fn test_equip_auto_swap() {
        let mut state = create_test_exploration_state();
        // Create two weapons
        let id1 = state.world.items.keys().max().unwrap() + 1;
        let id2 = id1 + 1;
        state.world.items.insert(id1, state::Item {
            id: id1, name: "Shortsword".to_string(), description: "A short sword.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6, damage_type: state::DamageType::Piercing,
                properties: 0, category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
        });
        state.world.items.insert(id2, state::Item {
            id: id2, name: "Longsword".to_string(), description: "A long sword.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 8, damage_type: state::DamageType::Slashing,
                properties: 0, category: state::WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
        });
        state.character.inventory.push(id1);
        state.character.inventory.push(id2);
        state.character.equipped.main_hand = Some(id1);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "equip longsword");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.equipped.main_hand, Some(id2), "Should swap to longsword");
        // Old weapon still in inventory
        assert!(new_state.character.inventory.contains(&id1));
    }

    #[test]
    fn test_unequip_weapon() {
        let mut state = create_test_exploration_state();
        let weapon_id = state.world.items.iter()
            .find(|(_, item)| matches!(item.item_type, state::ItemType::Weapon { .. }))
            .map(|(&id, _)| id).unwrap();
        let weapon_name = state.world.items[&weapon_id].name.clone();
        state.world.items.get_mut(&weapon_id).unwrap().carried_by_player = true;
        state.character.inventory.push(weapon_id);
        state.character.equipped.main_hand = Some(weapon_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, &format!("unequip {}", weapon_name.to_lowercase()));
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.character.equipped.main_hand.is_none());
        // Item still in inventory
        assert!(new_state.character.inventory.contains(&weapon_id));
    }

    #[test]
    fn test_character_sheet_shows_ac() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "character");
        assert!(output.text.iter().any(|t| t.contains("AC:")), "Character sheet should show AC. Got: {:?}", output.text);
    }

    #[test]
    fn test_inventory_shows_equipped() {
        let mut state = create_test_exploration_state();
        let weapon_id = state.world.items.iter()
            .find(|(_, item)| matches!(item.item_type, state::ItemType::Weapon { .. }))
            .map(|(&id, _)| id).unwrap();
        state.world.items.get_mut(&weapon_id).unwrap().carried_by_player = true;
        state.character.inventory.push(weapon_id);
        state.character.equipped.main_hand = Some(weapon_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "inventory");
        assert!(output.text.iter().any(|t| t.contains("equipped")), "Inventory should mark equipped items. Got: {:?}", output.text);
    }

    fn create_test_combat_state() -> GameState {
        let mut state = create_test_exploration_state();
        // Add a hostile goblin with combat stats to current location
        let npc_id = 100;
        let loc_id = state.current_location;
        state.world.npcs.insert(npc_id, state::Npc {
            id: npc_id,
            name: "Test Goblin".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: loc_id,
            combat_stats: Some(state::CombatStats {
                max_hp: 7, current_hp: 7, ac: 15, speed: 30,
                ability_scores: {
                    let mut m = HashMap::new();
                    m.insert(Ability::Strength, 8);
                    m.insert(Ability::Dexterity, 14);
                    m
                },
                attacks: vec![state::NpcAttack {
                    name: "Scimitar".to_string(), hit_bonus: 4,
                    damage_dice: 1, damage_die: 6, damage_bonus: 2,
                    damage_type: state::DamageType::Slashing, reach: 5,
                    range_normal: 0, range_long: 0,
                }],
                proficiency_bonus: 2,
            }),
            conditions: Vec::new(),
        });
        if let Some(loc) = state.world.locations.get_mut(&loc_id) {
            loc.npcs.push(npc_id);
        }

        // Give player a weapon
        let weapon_id = 200;
        state.world.items.insert(weapon_id, state::Item {
            id: weapon_id,
            name: "Longsword".to_string(),
            description: "A fine longsword.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 8,
                damage_type: state::DamageType::Slashing,
                properties: crate::equipment::VERSATILE,
                category: state::WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
        });
        state.character.inventory.push(weapon_id);
        state.character.equipped.main_hand = Some(weapon_id);

        // Start combat
        let mut rng = rand::rngs::StdRng::seed_from_u64(state.rng_seed + state.rng_counter);
        state.rng_counter += 1;
        let combat_state = combat::start_combat(&mut rng, &state.character, &[npc_id], &state.world.npcs);
        state.active_combat = Some(combat_state);
        state
    }

    fn force_player_turn(state: &mut GameState) {
        if let Some(ref mut combat) = state.active_combat {
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if *c == combat::Combatant::Player {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.player_action_used = false;
            combat.player_movement_remaining = state.character.speed;
        }
    }

    #[test]
    fn test_combat_blocks_exploration_commands() {
        let state = create_test_combat_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let go_output = process_input(&state_json, "go north");
        assert!(go_output.text.iter().any(|t| t.contains("can't do that during combat")),
            "Go should be blocked. Got: {:?}", go_output.text);

        let take_output = process_input(&state_json, "take sword");
        assert!(take_output.text.iter().any(|t| t.contains("can't do that during combat")),
            "Take should be blocked. Got: {:?}", take_output.text);
    }

    #[test]
    fn test_combat_allows_look() {
        let state = create_test_combat_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "look");
        assert!(output.text.iter().any(|t| t.contains("Combat")),
            "Look should show combat status. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_allows_inventory() {
        let state = create_test_combat_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "inventory");
        assert!(output.text.iter().any(|t| t.contains("carrying")),
            "Inventory should work in combat. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_allows_help() {
        let state = create_test_combat_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "help");
        assert!(output.text.iter().any(|t| t.contains("attack")),
            "Help should show combat commands. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_dodge_action() {
        let mut state = create_test_combat_state();
        // Ensure it's player's turn
        if let Some(ref mut combat) = state.active_combat {
            // Force player turn
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if *c == combat::Combatant::Player {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.player_action_used = false;
        }
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "dodge");
        assert!(output.text.iter().any(|t| t.contains("Dodge")),
            "Should confirm dodge. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_disengage_action() {
        let mut state = create_test_combat_state();
        if let Some(ref mut combat) = state.active_combat {
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if *c == combat::Combatant::Player {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.player_action_used = false;
        }
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "disengage");
        assert!(output.text.iter().any(|t| t.contains("Disengage")),
            "Should confirm disengage. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_dash_action() {
        let mut state = create_test_combat_state();
        if let Some(ref mut combat) = state.active_combat {
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if *c == combat::Combatant::Player {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.player_action_used = false;
        }
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "dash");
        assert!(output.text.iter().any(|t| t.contains("Dash")),
            "Should confirm dash. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_approach_keeps_player_turn_open() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 30);
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "approach test goblin");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();

        assert!(combat.is_player_turn(), "Approach should not auto-end turn");
        assert!(combat.player_movement_remaining < state.character.speed,
            "Movement should stay partially spent on same turn, got {}",
            combat.player_movement_remaining);
        assert!(output.text.iter().any(|t| t.contains("Your turn!")),
            "Expected turn prompt, got: {:?}", output.text);
    }

    #[test]
    fn test_combat_attack_keeps_turn_open_when_movement_remaining() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.player_movement_remaining = 30;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();

        assert!(combat.is_player_turn(), "Attack should not auto-end turn when movement remains");
        assert!(combat.player_action_used, "Attack should consume action");
    }

    #[test]
    fn test_combat_end_turn_advances_to_npc_and_back() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "end turn");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.is_player_turn(), "After NPC cycle, control should return to player");
        assert!(output.text.iter().any(|t| t.contains("You end your turn.")),
            "Expected end-turn confirmation, got: {:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("Your turn!")),
            "Expected player turn prompt after NPC turns, got: {:?}", output.text);
    }

    #[test]
    fn test_invalid_combat_command_does_not_consume_action_or_turn() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "dance wildly");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();

        assert!(combat.is_player_turn());
        assert!(!combat.player_action_used);
    }

    #[test]
    fn test_combat_ranged_attack_in_melee_has_disadvantage() {
        let mut state = create_test_combat_state();

        // Force player turn
        if let Some(ref mut combat) = state.active_combat {
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if *c == combat::Combatant::Player {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.player_action_used = false;
            combat.player_movement_remaining = 30;
            combat.distances.insert(100, 5); // target in melee range
        }

        // Equip a ranged weapon (shortbow)
        let bow_id = 201u32;
        state.world.items.insert(bow_id, state::Item {
            id: bow_id,
            name: "Shortbow".to_string(),
            description: "A shortbow.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1,
                damage_die: 6,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::AMMUNITION,
                category: state::WeaponCategory::Simple,
                versatile_die: 0,
                range_normal: 80,
                range_long: 320,
            },
            location: None,
            carried_by_player: true,
        });
        state.character.inventory.push(bow_id);
        state.character.equipped.main_hand = Some(bow_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");

        assert!(
            output.text.iter().any(|t| t.to_lowercase().contains("disadvantage")),
            "Expected disadvantage text for ranged attack in melee. Got: {:?}",
            output.text
        );
    }

    #[test]
    fn test_combat_not_in_combat_message() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack goblin");
        assert!(output.text.iter().any(|t| t.contains("not in combat")),
            "Should say not in combat. Got: {:?}", output.text);
    }

    #[test]
    fn test_combat_use_healing_potion_heals_and_consumes_action() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        state.character.current_hp = state.character.max_hp - 4;
        give_consumable_to_player(&mut state, "Healing Potion", "A potion.", "heal_1d8");

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use healing potion");

        assert!(output.text.iter().any(|t| t.contains("HP") || t.contains("recover")), "{:?}", output.text);

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.player_action_used, "Using potion should consume action in combat");
        assert!(new_state.character.current_hp > state.character.current_hp);
    }

    #[test]
    fn test_combat_use_after_action_is_blocked_without_consuming_item() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        give_consumable_to_player(&mut state, "Healing Potion", "A potion.", "heal_1d8");

        let first_json = serde_json::to_string(&state).unwrap();
        // Use dodge to consume action without ending combat (no damage dealt)
        let after_dodge = process_input(&first_json, "dodge");
        // Now try to use potion - should be blocked
        let after_use = process_input(&after_dodge.state_json, "use healing potion");

        assert!(after_use.text.iter().any(|t| t.contains("already used your action")));

        let post: GameState = serde_json::from_str(&after_use.state_json).unwrap();
        assert!(post.character.inventory.iter().any(|id| {
            post.world.items.get(id).map(|i| i.name == "Healing Potion").unwrap_or(false)
        }));
    }

    #[test]
    fn test_defeat_state_blocks_regular_commands() {
        let mut state = create_test_combat_state();
        state.character.current_hp = 0;
        state.active_combat = None;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "go north");

        assert!(output.text.iter().any(|t| t.contains("GAME OVER")),
            "Expected GAME OVER status. Got: {:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("Load a previous save")),
            "Expected recovery options in output. Got: {:?}", output.text);
        assert_eq!(output.state_json, state_json);
        assert!(!output.state_changed);
    }

    #[test]
    fn test_end_combat_defeat_mentions_recovery_options() {
        let mut state = create_test_exploration_state();
        let lines = end_combat(&mut state, false);
        assert!(lines.iter().any(|line| line.contains("Load a previous save")));
    }

    // Hypothesis: When HP <= 0, process_input returns GAME OVER text without parsing
    // input, so "new game" / "restart" do nothing. Fix: parse input before the early
    // return and check for Command::NewGame to call new_game() with a fresh seed.
    #[test]
    fn test_new_game_command_on_death_screen() {
        let mut state = create_test_combat_state();
        state.character.current_hp = 0;
        state.active_combat = None;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "new game");

        // Should start a fresh game (character creation), not show GAME OVER
        assert!(!output.text.iter().any(|t| t.contains("GAME OVER")),
            "Expected new game, not GAME OVER. Got: {:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("Choose your race")),
            "Expected character creation prompt. Got: {:?}", output.text);

        // The returned state should be in CharacterCreation(ChooseRace)
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseRace));
    }

    #[test]
    fn test_restart_command_on_death_screen() {
        let mut state = create_test_combat_state();
        state.character.current_hp = 0;
        state.active_combat = None;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "restart");

        assert!(!output.text.iter().any(|t| t.contains("GAME OVER")),
            "Expected new game, not GAME OVER. Got: {:?}", output.text);
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseRace));
    }

    #[test]
    fn test_game_over_hint_mentions_new_game() {
        let mut state = create_test_combat_state();
        state.character.current_hp = 0;
        state.active_combat = None;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "go north");

        assert!(output.text.iter().any(|t| t.contains("new game")),
            "GAME OVER text should mention 'new game'. Got: {:?}", output.text);
    }

    #[test]
    fn test_hostile_npcs_get_combat_stats() {
        let state = create_test_exploration_state();
        let hostile_npcs: Vec<_> = state.world.npcs.values()
            .filter(|n| n.disposition == state::Disposition::Hostile)
            .collect();
        for npc in &hostile_npcs {
            assert!(npc.combat_stats.is_some(),
                "Hostile NPC '{}' should have combat stats", npc.name);
        }
    }

    fn give_consumable_to_player(state: &mut GameState, name: &str, description: &str, effect: &str) -> u32 {
        let item_id = (state.world.items.len() as u32) + 1000;
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: description.to_string(),
            item_type: state::ItemType::Consumable { effect: effect.to_string() },
            location: None,
            carried_by_player: true,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    #[test]
    fn test_use_healing_potion_restores_hp() {
        let mut state = create_test_exploration_state();
        state.character.current_hp = state.character.max_hp - 5;
        give_consumable_to_player(&mut state, "Healing Potion", "A potion.", "heal_1d8");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use healing potion");
        // Should mention healing / HP restored
        assert!(output.text.iter().any(|t| t.contains("HP") || t.contains("heal") || t.contains("hp")),
            "Should mention HP in output. Got: {:?}", output.text);
        // Item should be consumed
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.inventory.contains(&1000),
            "Healing Potion should be removed from inventory");
        assert!(!new_state.world.items.contains_key(&1000),
            "Healing Potion should be removed from world items");
    }

    #[test]
    fn test_use_healing_potion_caps_at_max_hp() {
        let mut state = create_test_exploration_state();
        // Only 1 HP missing — heal should cap at max
        state.character.current_hp = state.character.max_hp - 1;
        give_consumable_to_player(&mut state, "Healing Potion", "A potion.", "heal_1d8");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use healing potion");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.character.current_hp <= new_state.character.max_hp,
            "HP should not exceed max_hp");
    }

    #[test]
    fn test_use_torch_dark_to_dim() {
        let mut state = create_test_exploration_state();
        let loc_id = state.current_location;
        state.world.locations.get_mut(&loc_id).unwrap().light_level = state::LightLevel::Dark;
        give_consumable_to_player(&mut state, "Torch", "A torch.", "light");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use torch");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.world.locations[&loc_id].light_level, state::LightLevel::Dim,
            "Dark room should become Dim after using torch");
        assert!(!new_state.character.inventory.contains(&1000));
    }

    #[test]
    fn test_use_torch_dim_to_bright() {
        let mut state = create_test_exploration_state();
        let loc_id = state.current_location;
        state.world.locations.get_mut(&loc_id).unwrap().light_level = state::LightLevel::Dim;
        give_consumable_to_player(&mut state, "Torch", "A torch.", "light");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use torch");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.world.locations[&loc_id].light_level, state::LightLevel::Bright,
            "Dim room should become Bright after using torch");
    }

    #[test]
    fn test_use_torch_already_bright() {
        let mut state = create_test_exploration_state();
        let loc_id = state.current_location;
        state.world.locations.get_mut(&loc_id).unwrap().light_level = state::LightLevel::Bright;
        give_consumable_to_player(&mut state, "Torch", "A torch.", "light");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use torch");
        // Should inform player
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("bright") || t.to_lowercase().contains("already")),
            "Should mention already bright. Got: {:?}", output.text);
        // Still consumed
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.inventory.contains(&1000));
    }

    #[test]
    fn test_use_rations_nourish() {
        let mut state = create_test_exploration_state();
        give_consumable_to_player(&mut state, "Rations", "Food.", "nourish");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use rations");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("nourish") || t.to_lowercase().contains("food") || t.to_lowercase().contains("eat")),
            "Should mention nourishment. Got: {:?}", output.text);
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.inventory.contains(&1000));
    }

    #[test]
    fn test_use_non_consumable_item() {
        let mut state = create_test_exploration_state();
        // Add a misc item
        let item_id = 2000u32;
        let item = state::Item {
            id: item_id,
            name: "Old Coin".to_string(),
            description: "A coin.".to_string(),
            item_type: state::ItemType::Misc,
            location: None,
            carried_by_player: true,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use old coin");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("can't use")),
            "Should say can't use. Got: {:?}", output.text);
        // Item should NOT be consumed
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.character.inventory.contains(&item_id),
            "Non-consumable item should still be in inventory");
    }

    #[test]
    fn test_objective_command_shows_current_goal_in_exploration() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "objective");
        assert!(output.text.iter().any(|t| t.contains("Objective:")), "{:?}", output.text);
    }

    #[test]
    fn test_first_victory_marks_objective_complete() {
        let mut state = create_test_exploration_state();
        assert!(!state.progress.first_victory);

        let lines = end_combat(&mut state, true);

        assert!(state.progress.first_victory);
        assert!(lines.iter().any(|l| l.contains("Objective complete")), "{:?}", lines);
    }

    #[test]
    fn test_map_command_lists_discovered_locations_with_current_marker() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "map");

        assert!(output.text.iter().any(|t| t.contains("=== MAP ===")), "{:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("*")), "{:?}", output.text);
    }

    fn create_test_exploration_state() -> GameState {
        let mut rng = StdRng::seed_from_u64(42);
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 15);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 12);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);

        let character = create_character(
            "TestHero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![Skill::Athletics, Skill::Perception],
        );

        let world = world::generate_world(&mut rng, 15);

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: [0].into_iter().collect(),
            world,
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 100,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: state::ProgressState::default(),
        }
    }

    // Hypothesis (Bug 1): Traps deal zero damage because the Trigger struct had no
    // damage_on_failure field, and lib.rs never subtracted HP on trigger failure.
    // Fix: added damage_on_failure to Trigger, populated in trigger.rs, applied in lib.rs.
    #[test]
    fn test_trap_trigger_failure_applies_damage() {
        use crate::types::Direction;

        let mut state = create_test_exploration_state();
        let start_hp = state.character.current_hp;

        // Create a connected location with a trap trigger
        let target_loc_id = 999;
        let trigger_id = 888;
        let current = state.current_location;

        // Add exit from current location to target
        if let Some(loc) = state.world.locations.get_mut(&current) {
            loc.exits.insert(Direction::North, target_loc_id);
        }

        // Create target location with a trap trigger
        state.world.locations.insert(target_loc_id, state::Location {
            id: target_loc_id,
            name: "Trapped Room".to_string(),
            description: "A suspicious room.".to_string(),
            location_type: state::LocationType::Room,
            exits: {
                let mut m = HashMap::new();
                m.insert(Direction::South, current);
                m
            },
            npcs: vec![],
            items: vec![],
            triggers: vec![trigger_id],
            light_level: state::LightLevel::Bright,
        });

        // Create a trap trigger with DC 99 (guaranteed failure) and known damage
        state.world.triggers.insert(trigger_id, state::Trigger {
            id: trigger_id,
            location: target_loc_id,
            trigger_type: state::TriggerType::SavingThrow(Ability::Dexterity),
            dc: 99, // Impossible to pass
            success_text: "You dodge!".to_string(),
            failure_text: "A dart hits you!".to_string(),
            one_shot: true,
            damage_on_failure: 4,
        });

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "go north");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Verify damage was applied
        assert!(new_state.character.current_hp < start_hp,
            "HP should decrease after trap failure: before={}, after={}",
            start_hp, new_state.character.current_hp);
        assert_eq!(new_state.character.current_hp, start_hp - 4,
            "HP should decrease by exactly 4 (the trap damage)");

        // Verify the output text mentions the damage
        let all_text = output.text.join("\n");
        assert!(all_text.contains("You take 4 damage!"),
            "Output should mention damage taken. Got: {}", all_text);
    }

    // Hypothesis (Bug 2): Dead enemies appear in look because narrate_look does not
    // filter out NPCs with combat_stats.current_hp <= 0.
    // Fix: added filter in narrate_look and narrate_enter_location.
    #[test]
    fn test_look_excludes_dead_npcs() {
        let mut state = create_test_exploration_state();
        let loc_id = state.current_location;

        // Add a dead hostile NPC to current location
        let dead_npc_id = 500;
        state.world.npcs.insert(dead_npc_id, state::Npc {
            id: dead_npc_id,
            name: "Dead Goblin".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: loc_id,
            combat_stats: Some(state::CombatStats {
                max_hp: 7,
                current_hp: 0, // Dead!
                ac: 15,
                speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
            }),
            conditions: vec![],
        });

        // Add a living friendly NPC
        let alive_npc_id = 501;
        state.world.npcs.insert(alive_npc_id, state::Npc {
            id: alive_npc_id,
            name: "Friendly Merchant".to_string(),
            role: state::NpcRole::Merchant,
            disposition: state::Disposition::Friendly,
            dialogue_tags: vec![],
            location: loc_id,
            combat_stats: None, // No combat stats (friendly)
            conditions: vec![],
        });

        if let Some(loc) = state.world.locations.get_mut(&loc_id) {
            loc.npcs.push(dead_npc_id);
            loc.npcs.push(alive_npc_id);
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "look");
        let all_text = output.text.join("\n");

        assert!(!all_text.contains("Dead Goblin"),
            "Dead NPC should not appear in look output. Got: {}", all_text);
        assert!(all_text.contains("Friendly Merchant"),
            "Living friendly NPC should appear in look output. Got: {}", all_text);
    }

    fn create_test_wizard_state() -> GameState {
        let mut rng = StdRng::seed_from_u64(42);
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 8);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 15);
        scores.insert(Ability::Wisdom, 12);
        scores.insert(Ability::Charisma, 10);

        let character = create_character(
            "Gandalf".to_string(),
            character::race::Race::Human,
            character::class::Class::Wizard,
            scores,
            vec![Skill::Arcana, Skill::Investigation],
        );

        let world = world::generate_world(&mut rng, 15);

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: [0].into_iter().collect(),
            world,
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 100,
            game_phase: GamePhase::Exploration,
            active_combat: None,
            ironman_mode: false,
            progress: state::ProgressState::default(),
        }
    }

    #[test]
    fn test_spells_command_wizard_shows_known_spells() {
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let all_text = output.text.join("\n");

        assert!(all_text.contains("Known Spells"), "Should have header. Got: {}", all_text);
        assert!(all_text.contains("Cantrips (at will)"), "Should list cantrips. Got: {}", all_text);
        assert!(all_text.contains("Fire Bolt"), "Should list Fire Bolt. Got: {}", all_text);
        assert!(all_text.contains("Prestidigitation"), "Should list Prestidigitation. Got: {}", all_text);
        assert!(all_text.contains("Level 1 Spells"), "Should list level 1 spells. Got: {}", all_text);
        assert!(all_text.contains("Magic Missile"), "Should list Magic Missile. Got: {}", all_text);
        assert!(all_text.contains("Burning Hands"), "Should list Burning Hands. Got: {}", all_text);
        assert!(all_text.contains("Sleep"), "Should list Sleep. Got: {}", all_text);
        assert!(all_text.contains("Shield"), "Should list Shield. Got: {}", all_text);
    }

    #[test]
    fn test_spells_command_wizard_shows_spell_slots() {
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let all_text = output.text.join("\n");

        assert!(all_text.contains("Spell Slots"), "Should show spell slots section. Got: {}", all_text);
        assert!(all_text.contains("Level 1: 2/2"), "Should show 2/2 level 1 slots. Got: {}", all_text);
    }

    #[test]
    fn test_spells_command_non_wizard_no_spells() {
        let state = create_test_exploration_state(); // Fighter
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let all_text = output.text.join("\n");

        assert!(all_text.contains("You don't know any spells"), "Non-wizard should get no-spells message. Got: {}", all_text);
    }

    #[test]
    fn test_spells_command_aliases_integration() {
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();

        for alias in &["spells", "spell list", "known spells", "my spells"] {
            let output = process_input(&state_json, alias);
            let all_text = output.text.join("\n");
            assert!(all_text.contains("Known Spells"),
                "'{}' should show known spells. Got: {}", alias, all_text);
        }
    }

    #[test]
    fn test_spells_command_does_not_mutate_state() {
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");

        // The spells command is read-only, state should not be marked as changed
        // (except for the rng_counter increment which is standard)
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.spell_slots_remaining, state.character.spell_slots_remaining);
        assert_eq!(new_state.character.known_spells, state.character.known_spells);
    }
}
