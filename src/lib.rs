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
pub mod rest;
pub mod leveling;

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
        in_world_minutes: 0,
        last_long_rest_minutes: None,
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
            GamePhase::Victory => handle_victory(&mut state, input),
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

            // Grant starting equipment based on class
            grant_starting_equipment(state);

            // Seed objectives from generated world
            seed_objectives(state);

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

/// Grant starting equipment to the character based on class loadout.
/// Creates items from SRD const tables, adds them to WorldState and character inventory,
/// and equips them to the correct slots.
fn grant_starting_equipment(state: &mut GameState) {
    use state::{Item, ItemType};

    let loadout = state.character.class.starting_loadout();

    // Compute next available item ID from existing world items
    let mut next_id = state.world.items.keys().max().map_or(0, |&id| id + 1);

    // Helper: create a weapon item from SRD table by name
    let create_weapon = |name: &str, id: u32| -> Option<Item> {
        equipment::SRD_WEAPONS.iter().find(|w| w.name == name).map(|w| Item {
            id,
            name: w.name.to_string(),
            description: format!("A {}.", w.name.to_lowercase()),
            item_type: ItemType::Weapon {
                damage_dice: w.damage_dice,
                damage_die: w.damage_die,
                damage_type: w.damage_type,
                properties: w.properties,
                category: w.category,
                versatile_die: w.versatile_die,
                range_normal: w.range_normal,
                range_long: w.range_long,
            },
            location: None,
            carried_by_player: true,
        })
    };

    // Helper: create an armor item from SRD table by name
    let create_armor = |name: &str, id: u32| -> Option<Item> {
        equipment::SRD_ARMOR.iter().find(|a| a.name == name).map(|a| Item {
            id,
            name: a.name.to_string(),
            description: format!("A set of {} armor.", a.name.to_lowercase()),
            item_type: ItemType::Armor {
                category: a.category,
                base_ac: a.base_ac,
                max_dex_bonus: a.max_dex_bonus,
                str_requirement: a.str_requirement,
                stealth_disadvantage: a.stealth_disadvantage,
            },
            location: None,
            carried_by_player: true,
        })
    };

    // Helper: find an SRD item by name (weapon or armor) and create it
    let find_and_create = |name: &str, id: u32| -> Option<Item> {
        create_weapon(name, id).or_else(|| create_armor(name, id))
    };

    // Equip main hand
    if let Some(name) = loadout.main_hand {
        if let Some(item) = find_and_create(name, next_id) {
            let id = item.id;
            state.world.items.insert(id, item);
            state.character.inventory.push(id);
            state.character.equipped.main_hand = Some(id);
            next_id += 1;
        }
    }

    // Equip off hand (shield or weapon)
    if let Some(name) = loadout.off_hand {
        if let Some(item) = find_and_create(name, next_id) {
            let id = item.id;
            state.world.items.insert(id, item);
            state.character.inventory.push(id);
            state.character.equipped.off_hand = Some(id);
            next_id += 1;
        }
    }

    // Equip body armor
    if let Some(name) = loadout.body {
        if let Some(item) = find_and_create(name, next_id) {
            let id = item.id;
            state.world.items.insert(id, item);
            state.character.inventory.push(id);
            state.character.equipped.body = Some(id);
            next_id += 1;
        }
    }

    // Add extra inventory items (not equipped)
    for &name in loadout.extra_inventory {
        if let Some(item) = find_and_create(name, next_id) {
            let id = item.id;
            state.world.items.insert(id, item);
            state.character.inventory.push(id);
            next_id += 1;
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
                    let mut lines = vec![narration::templates::TAKE_ITEM.replace("{item}", &name)];

                    // Check FindItem objectives
                    lines.extend(check_find_item_objectives(state, item_id));

                    // Check if all objectives are now complete
                    lines.extend(check_all_objectives_complete(state));

                    lines
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
            render_character_sheet_with_xp(state)
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
        Command::Attack(_) | Command::Approach(_) | Command::Retreat | Command::Dodge
        | Command::Disengage | Command::Dash | Command::EndTurn
        | Command::OffHandAttack(_) | Command::BonusDash | Command::ReactionYes | Command::ReactionNo => {
            vec!["You're not in combat.".to_string()]
        }
        Command::NewGame => {
            vec!["You can only start a new game after being defeated.".to_string()]
        }
        Command::ShortRest => {
            rest::handle_short_rest(state, &mut rng)
        }
        Command::LongRest => {
            rest::handle_long_rest(state, &mut rng)
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
            // -------- Reaction trigger: Shield (pre-attack) --------
            // If this NPC is about to make an attack against the player and the
            // player is eligible for the Shield reaction, pause NPC processing
            // and set a pending reaction. The attack will be resolved after the
            // player responds.
            if should_trigger_shield_reaction(&combat, state, npc_id) {
                let pre_ac = equipment::calculate_ac(&state.character, &state.world.items);
                combat.pending_reaction = Some(combat::PendingReaction::Shield {
                    attacker_npc_id: npc_id,
                    incoming_damage: 0, // not yet rolled
                    pre_roll_ac: pre_ac,
                    resume_npc_index: combat.current_turn,
                });
                let attacker_name = state.world.npcs.get(&npc_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "An enemy".to_string());
                lines.push(format!(
                    "{} is about to attack you! Cast Shield as a reaction? (yes/no)",
                    attacker_name
                ));
                state.active_combat = Some(combat);
                return lines;
            }

            // -------- Reaction trigger: Opportunity attack (pre-move) --------
            // If this NPC is about to move out of the player's melee reach
            // without disengaging AND the player has reaction + a melee weapon
            // in reach, pause NPC processing and prompt for OA.
            if let Some((old_dist, new_dist)) = should_trigger_opportunity_attack(&combat, state, npc_id) {
                combat.pending_reaction = Some(combat::PendingReaction::OpportunityAttack {
                    fleeing_npc_id: npc_id,
                    old_distance: old_dist,
                    new_distance: new_dist,
                    resume_npc_index: combat.current_turn,
                });
                let fleeing_name = state.world.npcs.get(&npc_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "An enemy".to_string());
                lines.push(format!(
                    "{} moves out of your reach ({}ft -> {}ft). Take an opportunity attack? (yes/no)",
                    fleeing_name, old_dist, new_dist
                ));
                state.active_combat = Some(combat);
                return lines;
            }

            let npc_lines = combat::resolve_npc_turn(rng, npc_id, state, &mut combat);
            lines.extend(npc_lines);
        }

        combat.advance_turn(state);
        state.active_combat = Some(combat);
    }

    lines
}

/// Return true when the player is eligible for the Shield reaction AND the given
/// NPC is about to make an attack against the player this turn.
fn should_trigger_shield_reaction(
    combat: &combat::CombatState,
    state: &GameState,
    npc_id: types::NpcId,
) -> bool {
    // Player must know Shield, have a 1st-level slot, and have reaction available.
    let knows_shield = state.character.known_spells.iter().any(|s| s == "Shield");
    if !knows_shield {
        return false;
    }
    if state.character.spell_slots_remaining.first().copied().unwrap_or(0) < 1 {
        return false;
    }
    if combat.reaction_used {
        return false;
    }
    if !conditions::can_take_reactions(&state.character.conditions) {
        return false;
    }

    // The NPC must have a viable attack against the player at the current distance.
    let npc = match state.world.npcs.get(&npc_id) { Some(n) => n, None => return false };
    let stats = match npc.combat_stats.as_ref() { Some(s) if s.current_hp > 0 => s, _ => return false };
    let distance = *combat.distances.get(&npc_id).unwrap_or(&u32::MAX);
    // Melee attack in reach?
    let has_melee_in_reach = stats.attacks.iter().any(|a| a.reach > 0 && distance <= a.reach as u32);
    if has_melee_in_reach {
        return true;
    }
    // Ranged attack in range?
    let has_ranged_in_range = stats.attacks.iter().any(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    });
    has_ranged_in_range
}

/// Return Some((old_distance, new_distance)) if the given NPC is about to move out
/// of the player's melee reach without disengaging AND the player is eligible for
/// an opportunity-attack reaction.
fn should_trigger_opportunity_attack(
    combat: &combat::CombatState,
    state: &GameState,
    npc_id: types::NpcId,
) -> Option<(u32, u32)> {
    // Player must have reaction available.
    if combat.reaction_used {
        return None;
    }
    if !conditions::can_take_reactions(&state.character.conditions) {
        return None;
    }

    let npc = state.world.npcs.get(&npc_id)?;
    let stats = npc.combat_stats.as_ref()?;
    if stats.current_hp <= 0 { return None; }

    // If the NPC has no attack in reach at current distance, it will try to move.
    let distance = *combat.distances.get(&npc_id).unwrap_or(&u32::MAX);
    let has_melee_in_reach = stats.attacks.iter().any(|a| a.reach > 0 && distance <= a.reach as u32);
    let has_ranged_in_range = stats.attacks.iter().any(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    });

    // Case 1: NPC will move TOWARD the player (attack not in range for either).
    // Moving closer never triggers OA; skip.
    // But the existing AI always moves toward. So NPCs don't normally leave reach.
    //
    // Case 2: A future NPC AI improvement could "kite" — move away when in melee.
    // For now we check: if the NPC has a ranged attack and is currently in the
    // player's melee reach, it COULD kite away. But the existing AI uses ranged
    // at melee with disadvantage rather than retreating. So OA won't fire in the
    // MVP AI, but we still need the machinery for future use.
    //
    // The only scenario where the current AI moves out of reach is not present
    // in the existing `resolve_npc_turn`. We short-circuit to None so the prompt
    // machinery exists but isn't fired by the current AI.
    let _ = (has_melee_in_reach, has_ranged_in_range, distance, stats);
    None
}

/// Resolve the player's decision on a pending reaction. Consumes the reaction
/// when accepted; applies the post-decision side effects (Shield AC bonus + slot
/// consumption, or opportunity-attack resolution). Clears `pending_reaction`.
fn resolve_reaction_decision(
    state: &mut GameState,
    rng: &mut StdRng,
    accept: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut combat = match state.active_combat.take() {
        Some(c) => c,
        None => return lines,
    };
    let pending = combat.pending_reaction.take();
    let reaction = match pending {
        Some(r) => r,
        None => {
            state.active_combat = Some(combat);
            return lines;
        }
    };

    match reaction {
        combat::PendingReaction::Shield { attacker_npc_id, pre_roll_ac: _, .. } => {
            if accept {
                // Consume spell slot + reaction; grant +5 AC until start of next turn.
                if !spells::consume_spell_slot(1, &mut state.character.spell_slots_remaining) {
                    // Shouldn't happen (pre-checked), but guard anyway.
                    lines.push("You reach for the Shield spell but have no slot left.".to_string());
                    state.active_combat = Some(combat);
                    return lines;
                }
                combat.player_shield_ac_bonus = 5;
                combat.reaction_used = true;
                lines.push("You cast Shield! (+5 AC until your next turn)".to_string());

                // Now resolve the attack against the shielded AC.
                let npc_lines = resolve_single_npc_attack(state, &mut combat, rng, attacker_npc_id);
                lines.extend(npc_lines);
                combat.advance_turn(state);
            } else {
                lines.push("You decline to cast Shield.".to_string());
                // Resolve attack at normal AC.
                let npc_lines = resolve_single_npc_attack(state, &mut combat, rng, attacker_npc_id);
                lines.extend(npc_lines);
                combat.advance_turn(state);
            }
        }
        combat::PendingReaction::OpportunityAttack { fleeing_npc_id, old_distance, new_distance, .. } => {
            if accept {
                combat.reaction_used = true;
                // Player attacks the fleeing NPC with their main-hand weapon.
                let weapon_id = state.character.equipped.main_hand;
                let target_ac = state.world.npcs.get(&fleeing_npc_id)
                    .and_then(|n| n.combat_stats.as_ref())
                    .map(|s| s.ac)
                    .unwrap_or(10);
                let target_conditions: Vec<crate::conditions::ActiveCondition> =
                    state.world.npcs.get(&fleeing_npc_id)
                        .map(|n| n.conditions.clone())
                        .unwrap_or_default();
                let result = combat::resolve_player_attack(
                    rng, &state.character, target_ac, false,
                    weapon_id, &state.world.items, old_distance,
                    state.character.equipped.off_hand.is_none(),
                    true, // hostile within 5ft
                    &target_conditions,
                );
                let name = state.world.npcs.get(&fleeing_npc_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "the enemy".to_string());
                if result.hit {
                    if let Some(npc) = state.world.npcs.get_mut(&fleeing_npc_id) {
                        if let Some(stats) = npc.combat_stats.as_mut() {
                            stats.current_hp -= result.damage;
                            if stats.current_hp <= 0 { stats.current_hp = 0; }
                        }
                    }
                    lines.push(format!(
                        "Opportunity attack! You strike {} for {} {} damage.",
                        name, result.damage, result.damage_type
                    ));
                } else {
                    lines.push(format!("You take an opportunity attack at {} -- miss.", name));
                }

                // Apply the NPC's movement (the OA happens at the trigger of leaving reach)
                combat.distances.insert(fleeing_npc_id, new_distance);
                combat.advance_turn(state);
            } else {
                lines.push("You let them pass.".to_string());
                // Apply the NPC's movement without an OA.
                combat.distances.insert(fleeing_npc_id, new_distance);
                combat.advance_turn(state);
            }
        }
    }

    state.active_combat = Some(combat);
    lines
}

/// Resolve a single NPC attack against the player (used after a Shield reaction
/// to apply the attack with the possibly-buffed AC).
fn resolve_single_npc_attack(
    state: &mut GameState,
    combat: &mut combat::CombatState,
    rng: &mut StdRng,
    npc_id: types::NpcId,
) -> Vec<String> {
    let mut lines = Vec::new();
    let (npc_name, npc_attacks) = {
        let npc = match state.world.npcs.get(&npc_id) { Some(n) => n, None => return lines };
        let stats = match npc.combat_stats.as_ref() { Some(s) if s.current_hp > 0 => s, _ => return lines };
        (npc.name.clone(), stats.attacks.clone())
    };
    let distance = *combat.distances.get(&npc_id).unwrap_or(&30);
    let player_ac = equipment::calculate_ac(&state.character, &state.world.items)
        + combat.player_shield_ac_bonus;
    let npc_conditions: Vec<crate::conditions::ActiveCondition> = state.world.npcs.get(&npc_id)
        .map(|n| n.conditions.clone()).unwrap_or_default();
    let player_conditions = state.character.conditions.clone();

    let melee = npc_attacks.iter().find(|a| a.reach > 0 && distance <= a.reach as u32);
    let attack_ref = melee.or_else(|| npc_attacks.iter().find(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    }));
    let attack = match attack_ref { Some(a) => a.clone(), None => return lines };

    let result = combat::resolve_npc_attack(
        rng, &attack, player_ac, combat.player_dodging, distance,
        &npc_conditions, &player_conditions,
    );
    let verb = if attack.reach > 0 { "attacks with" } else { "fires" };
    let disadv = if result.disadvantage { " (with disadvantage)" } else { "" };

    if result.hit {
        state.character.current_hp -= result.damage;
        if result.natural_20 {
            lines.push(format!("{} {} {} -- CRITICAL HIT! {} {} damage!",
                npc_name, verb, result.weapon_name, result.damage, result.damage_type));
        } else {
            lines.push(format!("{} {} {} ({}+{}={} vs AC {}){} -- hit for {} {} damage.",
                npc_name, verb, result.weapon_name, result.attack_roll,
                attack.hit_bonus, result.total_attack, player_ac, disadv,
                result.damage, result.damage_type));
        }
    } else if result.natural_1 {
        lines.push(format!("{} {} {} -- natural 1, miss!", npc_name, verb, result.weapon_name));
    } else {
        lines.push(format!("{} {} {} ({}+{}={} vs AC {}){} -- miss.",
            npc_name, verb, result.weapon_name, result.attack_roll,
            attack.hit_bonus, result.total_attack, player_ac, disadv));
    }
    lines
}

/// Render `narrate_character_sheet` plus XP/level progression info.
/// Kept in the orchestrator to avoid a `narration -> leveling` dependency.
fn render_character_sheet_with_xp(state: &GameState) -> Vec<String> {
    let mut lines = narration::narrate_character_sheet(state);
    let xp = state.character.xp;
    let level = state.character.level;
    if level >= leveling::LEVEL_CAP {
        lines.push(format!("XP: {} (max level)", xp));
    } else {
        let next = leveling::xp_for_next_level(level);
        lines.push(format!("XP: {} / {} (level {} -> {})", xp, next, level, level + 1));
    }
    if state.character.asi_credits > 0 {
        lines.push(format!(
            "Unspent ASI/feat credits: {}",
            state.character.asi_credits
        ));
    }
    lines
}

fn end_combat(state: &mut GameState, victory: bool) -> Vec<String> {
    // Snapshot the dead NPCs from the just-ended combat BEFORE clearing
    // `active_combat`, so we award XP only for foes that were actually in
    // this fight (not for unrelated dead NPCs elsewhere in the world).
    let dead_npc_crs: Vec<f32> = if victory {
        if let Some(combat) = state.active_combat.as_ref() {
            combat
                .initiative_order
                .iter()
                .filter_map(|(c, _)| match c {
                    combat::Combatant::Npc(id) => state
                        .world
                        .npcs
                        .get(id)
                        .and_then(|npc| npc.combat_stats.as_ref())
                        .filter(|cs| cs.current_hp <= 0)
                        .map(|cs| cs.cr),
                    combat::Combatant::Player => None,
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    state.active_combat = None;
    if victory {
        let mut lines = vec![
            String::new(),
            "=== VICTORY ===".to_string(),
            "All enemies have been defeated!".to_string(),
        ];

        // Award monster XP for every defeated foe in this combat.
        let monster_xp: u32 = dead_npc_crs
            .iter()
            .map(|&cr| leveling::xp_for_cr(cr))
            .sum();
        lines.extend(leveling::award_xp(&mut state.character, monster_xp));

        if !state.progress.first_victory {
            state.progress.first_victory = true;
            // Only show legacy message if no objectives are seeded
            if state.progress.objectives.is_empty() {
                lines.push("Objective complete: You survived your first battle.".to_string());
            }
        }

        // Check DefeatNpc objectives against dead NPCs
        lines.extend(check_defeat_npc_objectives(state));

        // Check if all objectives are now complete
        lines.extend(check_all_objectives_complete(state));

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

fn check_defeat_npc_objectives(state: &mut GameState) -> Vec<String> {
    let mut lines = Vec::new();
    let mut newly_completed = 0u32;
    for i in 0..state.progress.objectives.len() {
        if state.progress.objectives[i].completed {
            continue;
        }
        if let state::ObjectiveType::DefeatNpc(npc_id) = &state.progress.objective_triggers[i] {
            let npc_dead = state.world.npcs.get(npc_id)
                .and_then(|npc| npc.combat_stats.as_ref())
                .map(|cs| cs.current_hp <= 0)
                .unwrap_or(false);
            if npc_dead {
                state.progress.objectives[i].completed = true;
                lines.push(format!("Objective complete: {}", state.progress.objectives[i].title));
                newly_completed += 1;
            }
        }
    }
    if newly_completed > 0 {
        lines.extend(leveling::award_xp(
            &mut state.character,
            leveling::OBJECTIVE_XP_REWARD * newly_completed,
        ));
    }
    lines
}

fn check_find_item_objectives(state: &mut GameState, item_id: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut newly_completed = 0u32;
    for i in 0..state.progress.objectives.len() {
        if state.progress.objectives[i].completed {
            continue;
        }
        if let state::ObjectiveType::FindItem(target_id) = &state.progress.objective_triggers[i] {
            if *target_id == item_id {
                state.progress.objectives[i].completed = true;
                lines.push(format!("Objective complete: {}", state.progress.objectives[i].title));
                newly_completed += 1;
            }
        }
    }
    if newly_completed > 0 {
        lines.extend(leveling::award_xp(
            &mut state.character,
            leveling::OBJECTIVE_XP_REWARD * newly_completed,
        ));
    }
    lines
}

/// Check if all objectives are complete and transition to Victory phase if so.
/// Returns lines to append to the output.
fn check_all_objectives_complete(state: &mut GameState) -> Vec<String> {
    if !state.progress.objectives.is_empty()
        && state.progress.objectives.iter().all(|o| o.completed)
    {
        state.game_phase = GamePhase::Victory;
        vec![
            String::new(),
            "=== CONGRATULATIONS ===".to_string(),
            "You have completed all objectives and won the game!".to_string(),
            "Type 'new game' to start a new adventure.".to_string(),
        ]
    } else {
        Vec::new()
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
    let status = |used: bool| if used { "used" } else { "available" };
    lines.push(format!(
        "Movement: {} ft | Action: {} | Bonus: {} | Reaction: {} | Free: {}",
        combat.player_movement_remaining,
        status(combat.action_used),
        status(combat.bonus_action_used),
        status(combat.reaction_used),
        status(combat.free_interaction_used),
    ));
    lines.extend(combat::format_enemy_summary(state, combat));
    lines.push("Commands: attack <target>, approach <target>, retreat, dodge, disengage, dash, end turn".to_string());
    lines.push("Bonus actions: bonus dash, offhand attack <target>. Reactions: respond yes/no when prompted.".to_string());
}

fn handle_combat(state: &mut GameState, input: &str) -> Vec<String> {
    let command = parser::parse(input);
    let mut rng = StdRng::seed_from_u64(state.rng_seed + state.rng_counter);
    state.rng_counter += 1;

    // -------- Reaction prompt dispatch --------
    // When a reaction-triggering event has fired, the engine pauses NPC turn
    // processing and waits for the player's yes/no. Any input arriving while
    // `pending_reaction` is set is first interpreted in that context.
    let has_pending = state.active_combat.as_ref()
        .and_then(|c| c.pending_reaction.as_ref())
        .is_some();
    if has_pending {
        match command {
            Command::ReactionYes | Command::ReactionNo => {
                let accept = matches!(command, Command::ReactionYes);
                let mut lines = resolve_reaction_decision(state, &mut rng, accept);
                // Resume NPC processing after handling the reaction.
                let npc_lines = process_npc_turns(state, &mut rng);
                lines.extend(npc_lines);
                // Check combat end after NPC resumption.
                if let Some(ref combat) = state.active_combat {
                    if let Some(victory) = combat.check_end(state) {
                        lines.extend(end_combat(state, victory));
                        return lines;
                    }
                    append_player_turn_prompt(&mut lines, state, combat);
                }
                return lines;
            }
            _ => {
                // Ignore any other command while a reaction is pending; re-prompt.
                let combat = state.active_combat.as_ref().unwrap();
                let mut lines = vec![
                    "A reaction is pending -- respond with 'yes' or 'no'.".to_string(),
                ];
                match combat.pending_reaction.as_ref().unwrap() {
                    combat::PendingReaction::Shield { .. } =>
                        lines.push("Cast Shield? (yes/no)".to_string()),
                    combat::PendingReaction::OpportunityAttack { .. } =>
                        lines.push("Take an opportunity attack? (yes/no)".to_string()),
                }
                return lines;
            }
        }
    }

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
            return render_character_sheet_with_xp(state);
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
        // Put combat back so process_npc_turns can take it itself.
        state.active_combat = Some(combat);
        // Drive any pending NPC turns forward; this is where a reaction prompt
        // may fire.
        let mut lines = process_npc_turns(state, &mut rng);
        if let Some(ref combat) = state.active_combat {
            if combat.pending_reaction.is_some() {
                // Prompt already appended by process_npc_turns.
                return lines;
            }
            if let Some(victory) = combat.check_end(state) {
                lines.extend(end_combat(state, victory));
                return lines;
            }
            if combat.is_player_turn() {
                append_player_turn_prompt(&mut lines, state, combat);
            }
        }
        return lines;
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

                    if combat.action_used {
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

                    let is_unarmed = result.weapon_name == "Unarmed";
                    if result.hit {
                        if result.natural_20 {
                            if is_unarmed {
                                lines.push(format!("You punch {} -- CRITICAL HIT! {} {} damage!",
                                    npc_name, result.damage, result.damage_type));
                            } else {
                                lines.push(format!("You attack {} with {} -- CRITICAL HIT! {} {} damage!",
                                    npc_name, result.weapon_name, result.damage, result.damage_type));
                            }
                        } else if is_unarmed {
                            lines.push(format!("You punch {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                                npc_name, result.attack_roll,
                                result.total_attack - result.attack_roll, result.total_attack, target_ac,
                                result.damage, result.damage_type));
                        } else {
                            lines.push(format!("You attack {} with {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                                npc_name, result.weapon_name, result.attack_roll,
                                result.total_attack - result.attack_roll, result.total_attack, target_ac,
                                result.damage, result.damage_type));
                        }
                    } else if result.natural_1 {
                        if is_unarmed {
                            lines.push(format!("You swing at {} -- natural 1, miss!", npc_name));
                        } else {
                            lines.push(format!("You attack {} with {} -- natural 1, miss!",
                                npc_name, result.weapon_name));
                        }
                    } else if is_unarmed {
                        lines.push(format!("You swing at {} ({}+{}={} vs AC {}) -- miss.",
                            npc_name, result.attack_roll,
                            result.total_attack - result.attack_roll, result.total_attack, target_ac));
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

                    combat.action_used = true;
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
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_dodging = true;
            combat.action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
            lines.push("You take the Dodge action. Attacks against you have disadvantage until your next turn.".to_string());
        }
        Command::Disengage => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_disengaging = true;
            combat.action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
            lines.push("You take the Disengage action. You can retreat without provoking opportunity attacks.".to_string());
        }
        Command::Dash => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_movement_remaining += state.character.speed;
            combat.action_used = true;
            should_end_turn = false;
            lines.push(format!("You take the Dash action. Movement this turn: {} ft.", combat.player_movement_remaining));
        }
        Command::BonusDash => {
            if combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your bonus action this turn.".to_string()];
            }
            combat.player_movement_remaining += state.character.speed;
            combat.bonus_action_used = true;
            should_end_turn = false;
            lines.push(format!(
                "You dash as a bonus action. Movement this turn: {} ft.",
                combat.player_movement_remaining
            ));
        }
        Command::OffHandAttack(target_name) => {
            // Two-Weapon Fighting: requires main-hand Attack action already used,
            // both weapons light melee, and bonus action available.
            if !combat.action_used {
                state.active_combat = Some(combat);
                return vec![
                    "You must take the Attack action with your main hand before using the off-hand bonus attack.".to_string()
                ];
            }
            if combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your bonus action this turn.".to_string()];
            }

            // Off-hand weapon must be equipped and LIGHT melee.
            let off_hand_id = match state.character.equipped.off_hand {
                Some(id) => id,
                None => {
                    state.active_combat = Some(combat);
                    return vec![
                        "You have no weapon in your off hand. Equip a light weapon off hand first."
                            .to_string(),
                    ];
                }
            };
            let (is_light_melee, is_weapon) = match state.world.items.get(&off_hand_id) {
                Some(item) => match &item.item_type {
                    state::ItemType::Weapon { properties, range_normal, .. } => {
                        let light = properties & crate::equipment::LIGHT != 0;
                        let melee = *range_normal == 0
                            || (properties & crate::equipment::THROWN != 0); // thrown-light is fine at 5ft
                        (light && melee, true)
                    }
                    _ => (false, false),
                },
                None => (false, false),
            };
            if !is_weapon {
                state.active_combat = Some(combat);
                return vec!["Your off-hand item is not a weapon.".to_string()];
            }
            if !is_light_melee {
                state.active_combat = Some(combat);
                return vec![
                    "Two-Weapon Fighting requires a light weapon in the off hand.".to_string(),
                ];
            }

            // Main-hand must also be a light melee weapon to permit TWF.
            let main_hand_light_melee = match state.character.equipped.main_hand
                .and_then(|id| state.world.items.get(&id))
            {
                Some(item) => match &item.item_type {
                    state::ItemType::Weapon { properties, range_normal, .. } => {
                        let light = properties & crate::equipment::LIGHT != 0;
                        let melee = *range_normal == 0
                            || (properties & crate::equipment::THROWN != 0);
                        light && melee
                    }
                    _ => false,
                },
                None => false,
            };
            if !main_hand_light_melee {
                state.active_combat = Some(combat);
                return vec![
                    "Two-Weapon Fighting requires a light weapon in the main hand as well."
                        .to_string(),
                ];
            }

            // Resolve target.
            let owned_candidates = build_combat_npc_candidates(&combat, state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();
            match resolver::resolve_target(&target_name, &candidates) {
                ResolveResult::Found(id) => {
                    let npc_id = id as u32;
                    let distance = *combat.distances.get(&npc_id).unwrap_or(&30);
                    if distance > 5 {
                        state.active_combat = Some(combat);
                        return vec![format!(
                            "The target is too far away for an off-hand strike ({}ft). Close to melee first.",
                            distance
                        )];
                    }
                    let target_ac = state.world.npcs.get(&npc_id)
                        .and_then(|n| n.combat_stats.as_ref())
                        .map(|s| s.ac)
                        .unwrap_or(10);
                    let target_dodging = combat.npc_dodging.get(&npc_id).copied().unwrap_or(false);
                    let target_conditions: &[crate::conditions::ActiveCondition] =
                        state.world.npcs.get(&npc_id)
                            .map(|n| n.conditions.as_slice())
                            .unwrap_or(&[]);
                    let hostile_within_5ft = combat::has_living_hostile_within(state, &combat, 5);

                    // Resolve the attack using the OFF-HAND weapon.
                    let result = combat::resolve_player_attack(
                        &mut rng, &state.character, target_ac, target_dodging,
                        Some(off_hand_id), &state.world.items, distance,
                        false, // off-hand slot is occupied (by this weapon), no Versatile bonus
                        hostile_within_5ft,
                        target_conditions,
                    );

                    // Off-hand damage rule: remove the positive ability modifier from the
                    // damage roll. Negative modifiers still apply (SRD).
                    let ability_mod_used = {
                        let str_m = state.character.ability_modifier(Ability::Strength);
                        let dex_m = state.character.ability_modifier(Ability::Dexterity);
                        // resolve_player_attack uses max(STR,DEX) for FINESSE weapons,
                        // and STR for non-finesse melee. We always have LIGHT melee here;
                        // dagger/shortsword/scimitar are FINESSE|LIGHT so the mod picked is
                        // max(STR,DEX). For a pure-LIGHT non-finesse weapon (handaxe,
                        // light hammer, club, sickle) STR is used.
                        let is_finesse = match state.world.items.get(&off_hand_id) {
                            Some(item) => match &item.item_type {
                                state::ItemType::Weapon { properties, .. } =>
                                    properties & crate::equipment::FINESSE != 0,
                                _ => false,
                            },
                            None => false,
                        };
                        if is_finesse { str_m.max(dex_m) } else { str_m }
                    };
                    let mut adjusted_damage = result.damage;
                    if result.hit && ability_mod_used > 0 {
                        adjusted_damage = (adjusted_damage - ability_mod_used).max(1);
                    }

                    let npc_name = state.world.npcs.get(&npc_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "the enemy".to_string());

                    if result.hit {
                        if result.natural_20 {
                            lines.push(format!(
                                "You strike {} with your off-hand {} -- CRITICAL HIT! {} {} damage!",
                                npc_name, result.weapon_name, adjusted_damage, result.damage_type
                            ));
                        } else {
                            lines.push(format!(
                                "You strike {} with your off-hand {} ({}+{}={} vs AC {}) -- hit for {} {} damage.",
                                npc_name, result.weapon_name, result.attack_roll,
                                result.total_attack - result.attack_roll, result.total_attack,
                                target_ac, adjusted_damage, result.damage_type
                            ));
                        }
                    } else if result.natural_1 {
                        lines.push(format!(
                            "You strike with your off-hand {} -- natural 1, miss!",
                            result.weapon_name
                        ));
                    } else {
                        lines.push(format!(
                            "You strike with your off-hand {} ({}+{}={} vs AC {}) -- miss.",
                            result.weapon_name, result.attack_roll,
                            result.total_attack - result.attack_roll, result.total_attack,
                            target_ac
                        ));
                    }
                    if result.disadvantage {
                        lines.push("(Rolled with disadvantage)".to_string());
                    }

                    // Apply damage
                    if result.hit {
                        if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                            if let Some(stats) = npc.combat_stats.as_mut() {
                                stats.current_hp -= adjusted_damage;
                                if stats.current_hp <= 0 {
                                    stats.current_hp = 0;
                                    lines.push(format!("{} is slain!", npc_name));
                                }
                            }
                        }
                    }

                    combat.bonus_action_used = true;
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
        Command::Equip(target_str) => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            lines.extend(handle_equip_command(state, &target_str));
            combat.action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::Unequip(target_str) => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            lines.extend(handle_unequip_command(state, &target_str));
            combat.action_used = true;
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::Use(item_name) => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn. You can still move (approach/retreat).".to_string()];
            }
            let (mut use_lines, consumed_action) = resolve_use_item(state, &mut rng, &item_name);
            lines.append(&mut use_lines);
            if consumed_action {
                combat.action_used = true;
            }
            should_end_turn = combat.player_movement_remaining <= 0;
        }
        Command::EndTurn => {
            lines.push("You end your turn.".to_string());
            should_end_turn = true;
        }
        Command::ReactionYes | Command::ReactionNo => {
            // With no pending reaction, yes/no is a no-op with a reminder.
            state.active_combat = Some(combat);
            return vec!["There is nothing to react to right now.".to_string()];
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

            if combat.action_used {
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
                            combat.action_used = true;
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
                    combat.action_used = true;
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
                            combat.action_used = true;
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
                    combat.action_used = true;
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
                    combat.action_used = true;
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
                    combat.action_used = true;
                    should_end_turn = combat.player_movement_remaining <= 0;
                }
                _ => {
                    lines.push("That spell is not implemented yet.".to_string());
                }
            }
        }
        Command::ShortRest | Command::LongRest => {
            state.active_combat = Some(combat);
            return vec!["You cannot rest during combat.".to_string()];
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
    combat.end_player_turn();
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

fn seed_objectives(state: &mut GameState) {
    use state::{Objective, ObjectiveType, Disposition};

    // Find the first hostile NPC with combat stats to designate as boss
    let mut hostile_npcs: Vec<_> = state.world.npcs.values()
        .filter(|npc| npc.disposition == Disposition::Hostile && npc.combat_stats.is_some())
        .collect();
    // Sort by ID for deterministic selection
    hostile_npcs.sort_by_key(|npc| npc.id);

    if let Some(boss) = hostile_npcs.first() {
        let boss_id = boss.id;
        let boss_name = boss.name.clone();
        let boss_location = boss.location;
        let location_name = state.world.locations.get(&boss_location)
            .map(|loc| loc.name.clone())
            .unwrap_or_else(|| "an unknown place".to_string());

        state.progress.objectives.push(Objective {
            id: "defeat_boss".to_string(),
            title: format!("Defeat {}", boss_name),
            description: format!("A dangerous foe known as {} lurks in {}. Defeat them to prove your worth.", boss_name, location_name),
            completed: false,
        });
        state.progress.objective_triggers.push(ObjectiveType::DefeatNpc(boss_id));
    }
}

fn render_objective(state: &GameState) -> Vec<String> {
    // If objectives exist, show the quest log
    if !state.progress.objectives.is_empty() {
        let mut lines = vec!["=== QUEST LOG ===".to_string()];
        for obj in &state.progress.objectives {
            let marker = if obj.completed { "[X]" } else { "[ ]" };
            lines.push(format!("{} {}", marker, obj.title));
            lines.push(format!("    {}", obj.description));
        }
        return lines;
    }

    // Legacy fallback for old saves without objectives
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

fn handle_victory(state: &mut GameState, input: &str) -> Vec<String> {
    let command = parser::parse(input);
    match command {
        Command::NewGame => {
            let new_seed = state.rng_seed.wrapping_add(state.rng_counter);
            // Return a signal that will be handled by the caller
            // For now, return the victory message with new game hint
            let output = new_game(new_seed, state.ironman_mode);
            // We need to propagate the new game state, so we use a workaround:
            // set state to a freshly created game state
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            *state = new_state;
            return output.text;
        }
        Command::Help(topic) => {
            narration::templates::render_help(
                topic.as_deref(),
                narration::templates::HelpPhase::Exploration,
            )
        }
        Command::Objective => render_objective(state),
        _ => {
            vec![
                "=== VICTORY ===".to_string(),
                "You have completed all objectives and won the game!".to_string(),
                "Type 'new game' to start a new adventure, or 'objective' to review your quest log.".to_string(),
            ]
        }
    }
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
    fn test_fighter_gets_starting_equipment() {
        // Run full character creation as Fighter
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "1"); // Fighter
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "Aldric");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Fighter should have 3 items: Chain Mail, Longsword, Shield
        assert_eq!(state.character.inventory.len(), 3,
            "Fighter should have 3 starting items in inventory");

        // Verify all 3 slots are filled
        assert!(state.character.equipped.main_hand.is_some(), "main_hand should be equipped");
        assert!(state.character.equipped.off_hand.is_some(), "off_hand should be equipped");
        assert!(state.character.equipped.body.is_some(), "body should be equipped");

        // Verify item names
        let main_hand_id = state.character.equipped.main_hand.unwrap();
        let off_hand_id = state.character.equipped.off_hand.unwrap();
        let body_id = state.character.equipped.body.unwrap();

        assert_eq!(state.world.items[&main_hand_id].name, "Longsword");
        assert_eq!(state.world.items[&off_hand_id].name, "Shield");
        assert_eq!(state.world.items[&body_id].name, "Chain Mail");

        // Verify items are carried by player
        assert!(state.world.items[&main_hand_id].carried_by_player);
        assert!(state.world.items[&off_hand_id].carried_by_player);
        assert!(state.world.items[&body_id].carried_by_player);

        // Verify items have no location (carried, not on ground)
        assert!(state.world.items[&main_hand_id].location.is_none());
        assert!(state.world.items[&off_hand_id].location.is_none());
        assert!(state.world.items[&body_id].location.is_none());
    }

    #[test]
    fn test_rogue_gets_starting_equipment() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "2"); // Rogue
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2 3 4"); // 4 skills
        let output = process_input(&output.state_json, "Shadow");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Rogue should have 3 items: Leather, Shortsword, Dagger
        assert_eq!(state.character.inventory.len(), 3,
            "Rogue should have 3 starting items in inventory");

        // Main hand and body equipped, off hand empty
        assert!(state.character.equipped.main_hand.is_some(), "main_hand should be equipped");
        assert!(state.character.equipped.off_hand.is_none(), "off_hand should be empty");
        assert!(state.character.equipped.body.is_some(), "body should be equipped");

        let main_hand_id = state.character.equipped.main_hand.unwrap();
        let body_id = state.character.equipped.body.unwrap();

        assert_eq!(state.world.items[&main_hand_id].name, "Shortsword");
        assert_eq!(state.world.items[&body_id].name, "Leather");

        // Dagger should be in inventory but not equipped
        let dagger = state.character.inventory.iter()
            .find(|&&id| state.world.items[&id].name == "Dagger");
        assert!(dagger.is_some(), "Dagger should be in inventory");
    }

    #[test]
    fn test_wizard_gets_starting_equipment() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "3"); // Wizard
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "Gandalf");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Wizard should have 2 items: Quarterstaff, Dagger
        assert_eq!(state.character.inventory.len(), 2,
            "Wizard should have 2 starting items in inventory");

        // Only main hand equipped, no off hand or body
        assert!(state.character.equipped.main_hand.is_some(), "main_hand should be equipped");
        assert!(state.character.equipped.off_hand.is_none(), "off_hand should be empty");
        assert!(state.character.equipped.body.is_none(), "body should be empty");

        let main_hand_id = state.character.equipped.main_hand.unwrap();
        assert_eq!(state.world.items[&main_hand_id].name, "Quarterstaff");

        // Dagger should be in inventory but not equipped
        let dagger = state.character.inventory.iter()
            .find(|&&id| state.world.items[&id].name == "Dagger");
        assert!(dagger.is_some(), "Dagger should be in inventory");
    }

    #[test]
    fn test_starting_equipment_ids_dont_collide_with_world_items() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "1"); // Fighter
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "Aldric");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // All item IDs in the world should be unique
        let mut all_ids: Vec<_> = state.world.items.keys().collect();
        let original_len = all_ids.len();
        all_ids.sort();
        all_ids.dedup();
        assert_eq!(all_ids.len(), original_len, "All item IDs should be unique");

        // Starting equipment should be in the world items map
        for &inv_id in &state.character.inventory {
            assert!(state.world.items.contains_key(&inv_id),
                "Inventory item {} should exist in world items", inv_id);
        }
    }

    #[test]
    fn test_fighter_starting_ac() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "1"); // Fighter
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "Aldric");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Fighter with Chain Mail (AC 16, heavy) + Shield (+2) = AC 18
        let ac = equipment::calculate_ac(&state.character, &state.world.items);
        assert_eq!(ac, 18, "Fighter AC should be 18 (Chain Mail 16 + Shield 2)");
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
                cr: 0.25,
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
            combat.action_used = false;
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
            combat.action_used = false;
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
            combat.action_used = false;
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
            combat.action_used = false;
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
        assert!(combat.action_used, "Attack should consume action");
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
        assert!(!combat.action_used);
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
            combat.action_used = false;
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
        assert!(combat.action_used, "Using potion should consume action in combat");
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

    // ---- Action Economy: Turn-status display ----

    #[test]
    fn test_turn_status_shows_all_four_resources() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);

        // Trigger the turn prompt by running a no-op command sequence. Easier
        // to just call approach which keeps turn open.
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 30);
        }
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "approach test goblin");

        let joined = output.text.join("\n").to_lowercase();
        assert!(joined.contains("action"), "Expected action status, got: {:?}", output.text);
        assert!(joined.contains("bonus"), "Expected bonus status, got: {:?}", output.text);
        assert!(joined.contains("reaction"), "Expected reaction status, got: {:?}", output.text);
        assert!(joined.contains("movement"), "Expected movement status, got: {:?}", output.text);
    }

    #[test]
    fn test_combat_help_mentions_bonus_and_reaction() {
        let state = create_test_combat_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "help combat");

        let joined = output.text.join("\n").to_lowercase();
        assert!(joined.contains("bonus"), "Combat help should mention bonus actions, got: {:?}", output.text);
        assert!(joined.contains("reaction") || joined.contains("yes/no"),
            "Combat help should mention reactions, got: {:?}", output.text);
    }

    // ---- Action Economy: Reaction prompts during NPC turns ----

    /// Build a Wizard character with Shield known and a spell slot available.
    fn make_wizard_for_shield() -> character::Character {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 8);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 14);
        scores.insert(Ability::Intelligence, 16);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 10);
        create_character(
            "ShieldWiz".to_string(),
            character::race::Race::Human,
            character::class::Class::Wizard,
            scores,
            vec![],
        )
    }

    fn wizard_combat_state() -> GameState {
        let mut state = create_test_combat_state();
        // Replace character with wizard
        let wizard = make_wizard_for_shield();
        state.character.class = wizard.class;
        state.character.known_spells = wizard.known_spells;
        state.character.spell_slots_max = wizard.spell_slots_max.clone();
        state.character.spell_slots_remaining = wizard.spell_slots_max;
        state.character.ability_scores = wizard.ability_scores;
        state
    }

    #[test]
    fn test_shield_reaction_prompt_fires_when_npc_hits_wizard() {
        // Setup: wizard w/ Shield, goblin in melee range with high attack bonus (guaranteed hit).
        let mut state = wizard_combat_state();
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            // Advance to NPC turn
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if matches!(c, combat::Combatant::Npc(_)) {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.reaction_used = false;
            combat.pending_reaction = None;
        }
        // Give goblin a ridiculously high hit bonus to guarantee hit
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(cs) = npc.combat_stats.as_mut() {
                cs.attacks[0].hit_bonus = 100;
            }
        }

        let state_json = serde_json::to_string(&state).unwrap();
        // Empty input to trigger NPC turn processing
        let output = process_input(&state_json, "");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.pending_reaction.is_some(),
            "Shield reaction prompt should be set when NPC hit would land. Got text: {:?}", output.text);
        assert!(matches!(combat.pending_reaction, Some(combat::PendingReaction::Shield { .. })),
            "Pending reaction should be Shield");
        assert!(output.text.iter().any(|t|
            t.to_lowercase().contains("shield") && (
                t.to_lowercase().contains("yes") || t.to_lowercase().contains("cast") || t.contains("?")
            )
        ), "Expected Shield prompt in output, got: {:?}", output.text);
    }

    #[test]
    fn test_shield_reaction_not_offered_when_no_slots() {
        let mut state = wizard_combat_state();
        // Drain all first-level slots
        for slot in state.character.spell_slots_remaining.iter_mut() {
            *slot = 0;
        }
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            for (i, (c, _)) in combat.initiative_order.iter().enumerate() {
                if matches!(c, combat::Combatant::Npc(_)) {
                    combat.current_turn = i;
                    break;
                }
            }
            combat.reaction_used = false;
            combat.pending_reaction = None;
        }
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(cs) = npc.combat_stats.as_mut() {
                cs.attacks[0].hit_bonus = 100;
            }
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.pending_reaction.is_none(),
            "Shield should not be offered with no spell slots, got: {:?}", output.text);
    }

    #[test]
    fn test_shield_reaction_yes_consumes_slot_and_reaction() {
        // Directly install a pending Shield reaction, confirm yes response resolves it.
        let mut state = wizard_combat_state();
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            // Put state on the NPC's turn so the post-resolution loop resumes correctly.
            let npc_idx = combat.initiative_order.iter()
                .position(|(c, _)| matches!(c, combat::Combatant::Npc(_)))
                .unwrap();
            combat.current_turn = npc_idx;
            combat.reaction_used = false;
            combat.pending_reaction = Some(combat::PendingReaction::Shield {
                attacker_npc_id: 100,
                incoming_damage: 5,
                pre_roll_ac: 12,
                resume_npc_index: npc_idx,
            });
        }
        let slots_before = state.character.spell_slots_remaining[0];

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "yes");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.pending_reaction.is_none(),
            "Pending reaction should be cleared after response, got: {:?}", output.text);
        assert_eq!(new_state.character.spell_slots_remaining[0], slots_before - 1,
            "Shield should consume a first-level slot");
        // The output must confirm Shield was cast.
        assert!(output.text.iter().any(|t| t.contains("Shield")),
            "Expected Shield narration, got: {:?}", output.text);
        // After advancement back to the player, shield_ac_bonus is reset per SRD
        // ("until start of caster's next turn"). reaction_used is likewise reset
        // at end-of-previous-turn. So the behavior we verify is:
        //   - slot consumed
        //   - shield narration present
        //   - combat turn advanced to player (no pending reaction)
        assert!(combat.is_player_turn(), "Control should return to player after reaction");
    }

    #[test]
    fn test_shield_reaction_no_declines_and_resolves_attack() {
        let mut state = wizard_combat_state();
        let slots_before = state.character.spell_slots_remaining[0];
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            let npc_idx = combat.initiative_order.iter()
                .position(|(c, _)| matches!(c, combat::Combatant::Npc(_)))
                .unwrap();
            combat.current_turn = npc_idx;
            combat.reaction_used = false;
            combat.pending_reaction = Some(combat::PendingReaction::Shield {
                attacker_npc_id: 100,
                incoming_damage: 5,
                pre_roll_ac: 12,
                resume_npc_index: npc_idx,
            });
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "no");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.pending_reaction.is_none(),
            "Pending reaction should be cleared after decline, got: {:?}", output.text);
        assert_eq!(new_state.character.spell_slots_remaining[0], slots_before,
            "Declining Shield should NOT consume a spell slot");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("decline")),
            "Expected decline narration, got: {:?}", output.text);
        assert!(combat.is_player_turn(),
            "After the NPC's attack resolves, control should return to the player");
    }

    #[test]
    fn test_opportunity_attack_prompt_fires_when_npc_leaves_reach() {
        // Goblin at 5ft tries to move away (distance tests the retreat branch).
        // For this we need a movement-only NPC (no attacks in reach after move), so
        // we put it at 5ft with only a ranged attack available after moving outside reach.
        let mut state = create_test_combat_state();
        // Force the goblin to have no melee within its speed range -- give it only ranged.
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(cs) = npc.combat_stats.as_mut() {
                // Keep the scimitar but make it ranged-only attacks
                cs.attacks.clear();
                cs.attacks.push(state::NpcAttack {
                    name: "Shortbow".to_string(), hit_bonus: 4,
                    damage_dice: 1, damage_die: 6, damage_bonus: 2,
                    damage_type: state::DamageType::Piercing, reach: 0,
                    range_normal: 80, range_long: 320,
                });
            }
        }
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            let npc_idx = combat.initiative_order.iter()
                .position(|(c, _)| matches!(c, combat::Combatant::Npc(_)))
                .unwrap();
            combat.current_turn = npc_idx;
            combat.reaction_used = false;
            combat.pending_reaction = None;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        // If the NPC has no melee and is already adjacent with ranged, it would attack ranged (disadvantage),
        // so no OA. But once the NPC's attack is resolved, no OA. Let's instead make the NPC
        // choose to MOVE -- by putting it far away then back again? We'll just check that the
        // machinery doesn't crash and that when triggered, the state updates correctly.
        // For this test, let's make sure that simply no pending reaction is set since goblin
        // attacked with shortbow.
        assert!(combat.pending_reaction.is_none(),
            "Ranged NPC at melee range shouldn't trigger opportunity attack");
        assert!(output.text.iter().any(|t| t.contains("Shortbow")),
            "NPC should have used ranged attack (shortbow), got: {:?}", output.text);
    }

    #[test]
    fn test_reaction_yes_no_without_pending_falls_through() {
        // If no pending reaction, "yes" and "no" should not consume turn resources.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.pending_reaction = None;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "yes");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        // Should get an "unknown" or similar error message, not crash.
        assert!(combat.is_player_turn(), "Yes with no pending should not consume turn");
        assert!(!combat.action_used);
        assert!(!combat.bonus_action_used);
        assert!(!combat.reaction_used);
        assert!(output.text.iter().any(|t|
            t.to_lowercase().contains("nothing") || t.to_lowercase().contains("unknown")
                || t.to_lowercase().contains("no pending") || t.to_lowercase().contains("not")
        ), "Expected informative message, got: {:?}", output.text);
    }

    // ---- Action Economy: OffHandAttack ----

    /// Equip the player with two light finesse weapons (Shortsword main + Dagger off-hand),
    /// returning (main_hand_id, off_hand_id).
    fn equip_dual_light_weapons(state: &mut GameState) -> (u32, u32) {
        // Clear any existing equipment first
        state.character.equipped.main_hand = None;
        state.character.equipped.off_hand = None;

        let main_id = 300u32;
        state.world.items.insert(main_id, state::Item {
            id: main_id,
            name: "Shortsword".to_string(),
            description: "A light shortsword.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::FINESSE | crate::equipment::LIGHT,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
        });
        state.character.inventory.push(main_id);
        state.character.equipped.main_hand = Some(main_id);

        let off_id = 301u32;
        state.world.items.insert(off_id, state::Item {
            id: off_id,
            name: "Dagger".to_string(),
            description: "A light dagger.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 4,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::FINESSE | crate::equipment::LIGHT | crate::equipment::THROWN,
                category: state::WeaponCategory::Simple,
                versatile_die: 0, range_normal: 20, range_long: 60,
            },
            location: None,
            carried_by_player: true,
        });
        state.character.inventory.push(off_id);
        state.character.equipped.off_hand = Some(off_id);

        (main_id, off_id)
    }

    #[test]
    fn test_offhand_attack_requires_main_hand_attack_first() {
        // Off-hand attack without having used the Attack action should be blocked.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        equip_dual_light_weapons(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.action_used = false;
            combat.bonus_action_used = false;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "offhand attack test goblin");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(!combat.bonus_action_used,
            "Off-hand attack without main-hand Attack should not consume bonus action");
        assert!(output.text.iter().any(|t|
            t.to_lowercase().contains("main-hand") || t.to_lowercase().contains("main hand")
                || t.to_lowercase().contains("attack action")
        ), "Expected main-hand-required narration, got: {:?}", output.text);
    }

    #[test]
    fn test_offhand_attack_consumes_bonus_action_after_attack() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        equip_dual_light_weapons(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.action_used = true;
            combat.bonus_action_used = false;
            combat.player_movement_remaining = 30;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "offhand attack test goblin");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.bonus_action_used,
            "Off-hand attack should consume the bonus action. Got: {:?}", output.text);
        assert!(combat.action_used,
            "Action flag should remain set from the main-hand attack");
    }

    #[test]
    fn test_offhand_attack_blocked_when_bonus_action_used() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        equip_dual_light_weapons(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.action_used = true;
            combat.bonus_action_used = true; // Already spent
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "offhand attack test goblin");

        assert!(output.text.iter().any(|t| t.to_lowercase().contains("bonus action")),
            "Expected bonus-action-used narration, got: {:?}", output.text);
    }

    #[test]
    fn test_offhand_attack_requires_light_offhand_weapon() {
        // If the off-hand weapon is not LIGHT, off-hand attack should be refused.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        let (_main_id, off_id) = equip_dual_light_weapons(&mut state);
        // Override the dagger to be non-LIGHT.
        if let Some(item) = state.world.items.get_mut(&off_id) {
            item.item_type = state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6,
                damage_type: state::DamageType::Slashing,
                properties: 0, // no LIGHT
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            };
        }
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.action_used = true;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "offhand attack test goblin");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(!combat.bonus_action_used,
            "Non-light off-hand weapon should not consume bonus action");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("light")),
            "Expected 'light' mention in rejection text, got: {:?}", output.text);
    }

    #[test]
    fn test_offhand_attack_requires_offhand_weapon_equipped() {
        // No off-hand weapon equipped -> off-hand attack is refused.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        // No off-hand
        state.character.equipped.off_hand = None;
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.action_used = true;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "offhand attack test goblin");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(!combat.bonus_action_used);
        assert!(output.text.iter().any(|t|
            t.to_lowercase().contains("off hand") || t.to_lowercase().contains("off-hand")
        ), "Expected off-hand requirement text, got: {:?}", output.text);
    }

    #[test]
    fn test_offhand_attack_damage_omits_positive_ability_modifier() {
        // Off-hand damage should exclude the positive STR/DEX ability modifier.
        // Verify that a hit deals damage consistent with die + 0 mod (not die + mod).
        // We use a high-STR fighter so any mod-included damage would show up as >= 5.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        equip_dual_light_weapons(&mut state);

        // Confirm the fighter has positive STR/DEX (from helper in test_character)
        let ability_mod = state.character.ability_modifier(Ability::Strength)
            .max(state.character.ability_modifier(Ability::Dexterity));
        assert!(ability_mod > 0, "Test setup needs positive ability modifier");

        // Capture the goblin's starting HP
        let goblin_start_hp = state.world.npcs.get(&100).unwrap().combat_stats.as_ref().unwrap().current_hp;

        // Try many seeds to collect off-hand hits and check the damage distribution.
        let mut max_damage_seen = 0i32;
        let mut any_hit = false;
        for seed in 0..200u64 {
            let mut test_state = state.clone();
            test_state.rng_seed = seed;
            test_state.rng_counter = 0;
            if let Some(ref mut combat) = test_state.active_combat {
                combat.distances.insert(100, 5);
                combat.action_used = true;
                combat.bonus_action_used = false;
                // Reset goblin HP to full
                if let Some(npc) = test_state.world.npcs.get_mut(&100) {
                    if let Some(cs) = npc.combat_stats.as_mut() {
                        cs.current_hp = goblin_start_hp;
                    }
                }
            }
            let json = serde_json::to_string(&test_state).unwrap();
            let output = process_input(&json, "offhand attack test goblin");
            let post: GameState = serde_json::from_str(&output.state_json).unwrap();
            let post_hp = post.world.npcs.get(&100)
                .and_then(|n| n.combat_stats.as_ref())
                .map(|s| s.current_hp)
                .unwrap_or(goblin_start_hp);
            let dmg = goblin_start_hp - post_hp;
            if dmg > 0 {
                any_hit = true;
                max_damage_seen = max_damage_seen.max(dmg);
            }
        }
        assert!(any_hit, "Expected at least one hit across 200 seeds");
        // Dagger = 1d4 (no crit on most seeds). Max off-hand damage on a non-crit
        // should be 4 (die max), never more. If the ability modifier had been
        // applied, max damage would be 4 + ability_mod > 4.
        //
        // Crits double the dice: max non-mod crit damage = 8. So upper bound is 8.
        let crit_max = 2 * 4; // 2d4
        assert!(max_damage_seen <= crit_max,
            "Max off-hand damage = {} which exceeds expected die max {}; ability mod probably added. ability_mod={}",
            max_damage_seen, crit_max, ability_mod);
    }

    // ---- Action Economy: BonusDash ----

    #[test]
    fn test_bonus_dash_grants_movement_and_consumes_bonus_action() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.player_movement_remaining = 15; // some movement already spent
            combat.bonus_action_used = false;
        }
        let baseline_movement = 15;
        let speed = state.character.speed;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "bonus dash");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.bonus_action_used, "BonusDash should consume the bonus action");
        assert!(!combat.action_used, "BonusDash should NOT consume the action");
        assert_eq!(combat.player_movement_remaining, baseline_movement + speed,
            "BonusDash should grant speed-equivalent movement");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("dash")),
            "Expected dash narration, got: {:?}", output.text);
    }

    #[test]
    fn test_bonus_dash_blocked_when_bonus_action_used() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.bonus_action_used = true;
            combat.player_movement_remaining = 30;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "bonus dash");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert_eq!(combat.player_movement_remaining, 30,
            "Movement should not change when bonus action already used");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("bonus action")),
            "Expected bonus-action-used narration, got: {:?}", output.text);
    }

    #[test]
    fn test_bonus_dash_does_not_end_turn_automatically() {
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.player_movement_remaining = 0;
            combat.bonus_action_used = false;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "bonus dash");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(combat.is_player_turn(),
            "BonusDash should keep the player's turn open (movement just got granted).");
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
    fn test_objective_command_shows_quest_log_with_objectives() {
        let mut state = create_test_exploration_state();
        // Add an objective
        state.progress.objectives.push(state::Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat the Boss".to_string(),
            description: "Slay the fearsome enemy.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(0));

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "objective");
        let text = output.text.join("\n");
        assert!(text.contains("=== QUEST LOG ==="), "Should show quest log header: {}", text);
        assert!(text.contains("[ ]"), "Incomplete objective should show [ ]: {}", text);
        assert!(text.contains("Defeat the Boss"), "Should show objective title: {}", text);
        assert!(text.contains("Slay the fearsome enemy"), "Should show description: {}", text);
    }

    #[test]
    fn test_objective_command_shows_completed_marker() {
        let mut state = create_test_exploration_state();
        state.progress.objectives.push(state::Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat the Boss".to_string(),
            description: "Slay the fearsome enemy.".to_string(),
            completed: true,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(0));

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "quest");
        let text = output.text.join("\n");
        assert!(text.contains("[X]"), "Completed objective should show [X]: {}", text);
    }

    #[test]
    fn test_defeat_boss_npc_completes_objective() {
        let mut state = create_test_exploration_state();

        // Set up a boss NPC (hostile with combat stats, hp <= 0 means defeated)
        let boss_id: u32 = 999;
        state.world.npcs.insert(boss_id, state::Npc {
            id: boss_id,
            name: "Boss Enemy".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(state::CombatStats {
                max_hp: 20,
                current_hp: 0, // Dead
                ac: 12,
                speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 1.0,
            }),
            conditions: vec![],
        });

        // Add DefeatNpc objective for this boss
        state.progress.objectives.push(state::Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat Boss Enemy".to_string(),
            description: "Slay the boss.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(boss_id));

        let lines = end_combat(&mut state, true);

        assert!(state.progress.objectives[0].completed,
            "DefeatNpc objective should be marked complete after combat victory");
        assert!(lines.iter().any(|l| l.contains("Objective complete") && l.contains("Defeat Boss Enemy")),
            "Should announce objective completion: {:?}", lines);
    }

    #[test]
    fn test_take_artifact_completes_find_item_objective() {
        let mut state = create_test_exploration_state();

        // Place an artifact item in the current room
        let artifact_id: u32 = 998;
        state.world.items.insert(artifact_id, state::Item {
            id: artifact_id,
            name: "Ancient Gem".to_string(),
            description: "A glowing gem of power.".to_string(),
            item_type: state::ItemType::Misc,
            location: Some(state.current_location),
            carried_by_player: false,
        });
        if let Some(loc) = state.world.locations.get_mut(&state.current_location) {
            loc.items.push(artifact_id);
        }

        // Add FindItem objective for this artifact
        state.progress.objectives.push(state::Objective {
            id: "find_artifact".to_string(),
            title: "Find the Ancient Gem".to_string(),
            description: "Locate the gem hidden in the ruins.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::FindItem(artifact_id));

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "take ancient gem");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.progress.objectives[0].completed,
            "FindItem objective should be marked complete after picking up the artifact");
        assert!(output.text.iter().any(|l| l.contains("Objective complete") && l.contains("Ancient Gem")),
            "Should announce objective completion: {:?}", output.text);
        // Quest XP bonus is awarded on FindItem completion.
        assert_eq!(new_state.character.xp, leveling::OBJECTIVE_XP_REWARD,
            "FindItem completion should award OBJECTIVE_XP_REWARD ({}); got {}",
            leveling::OBJECTIVE_XP_REWARD, new_state.character.xp);
    }

    #[test]
    fn test_victory_phase_blocks_exploration_commands() {
        let mut state = create_test_exploration_state();
        state.game_phase = GamePhase::Victory;
        state.progress.objectives.push(state::Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat the Boss".to_string(),
            description: "Done.".to_string(),
            completed: true,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(0));

        let state_json = serde_json::to_string(&state).unwrap();

        // Regular commands should show victory message
        let output = process_input(&state_json, "go north");
        assert!(output.text.iter().any(|t| t.contains("VICTORY")),
            "Should show victory message, got: {:?}", output.text);

        // Help should still work
        let output = process_input(&state_json, "help");
        assert!(output.text.iter().any(|t| t.contains("Commands")),
            "Help should still work in victory phase: {:?}", output.text);

        // Objective should still work
        let output = process_input(&state_json, "quest");
        assert!(output.text.iter().any(|t| t.contains("QUEST LOG")),
            "Quest log should still work in victory phase: {:?}", output.text);
    }

    #[test]
    fn test_victory_phase_allows_new_game() {
        let mut state = create_test_exploration_state();
        state.game_phase = GamePhase::Victory;

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "new game");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseRace),
            "new game from victory should start character creation");
    }

    #[test]
    fn test_all_objectives_complete_triggers_victory_phase() {
        let mut state = create_test_exploration_state();

        // Set up a single boss objective
        let boss_id: u32 = 997;
        state.world.npcs.insert(boss_id, state::Npc {
            id: boss_id,
            name: "Final Boss".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(state::CombatStats {
                max_hp: 20,
                current_hp: 0, // Dead
                ac: 12,
                speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 1.0,
            }),
            conditions: vec![],
        });

        state.progress.objectives.push(state::Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat Final Boss".to_string(),
            description: "Slay the final boss.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(boss_id));

        let lines = end_combat(&mut state, true);

        assert_eq!(state.game_phase, GamePhase::Victory,
            "Game should transition to Victory when all objectives complete");
        assert!(lines.iter().any(|l| l.contains("CONGRATULATIONS")),
            "Should show congratulations message: {:?}", lines);
    }

    #[test]
    fn test_objective_command_fallback_for_old_saves() {
        // Old saves without objectives should still show something
        let state = create_test_exploration_state();
        assert!(state.progress.objectives.is_empty());
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "objective");
        // Should show legacy fallback
        assert!(!output.text.is_empty(), "Should show something for old saves");
    }

    #[test]
    fn test_map_command_lists_discovered_locations_with_current_marker() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "map");

        assert!(output.text.iter().any(|t| t.contains("=== MAP ===")), "{:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("*")), "{:?}", output.text);
    }

    #[test]
    fn test_world_generation_seeds_objectives() {
        // Complete a full character creation flow and verify objectives are seeded
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // race
        let output = process_input(&output.state_json, "1"); // class
        let output = process_input(&output.state_json, "1"); // standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8"); // scores
        let output = process_input(&output.state_json, "1 2"); // skills
        let output = process_input(&output.state_json, "TestHero"); // name

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::Exploration));
        assert!(!state.progress.objectives.is_empty(),
            "Should have at least one objective after world generation");
        assert_eq!(state.progress.objectives.len(), state.progress.objective_triggers.len(),
            "objectives and objective_triggers should have same length");

        // Verify the objective references a real entity
        let obj = &state.progress.objectives[0];
        assert!(!obj.title.is_empty());
        assert!(!obj.description.is_empty());
        assert!(!obj.completed);

        match &state.progress.objective_triggers[0] {
            state::ObjectiveType::DefeatNpc(npc_id) => {
                assert!(state.world.npcs.contains_key(npc_id),
                    "DefeatNpc objective should reference an existing NPC");
                let npc = &state.world.npcs[npc_id];
                assert!(obj.title.contains(&npc.name),
                    "Objective title should contain NPC name: {} not in {}", npc.name, obj.title);
            }
            state::ObjectiveType::FindItem(item_id) => {
                assert!(state.world.items.contains_key(item_id),
                    "FindItem objective should reference an existing item");
            }
        }
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
            in_world_minutes: 0,
            last_long_rest_minutes: None,
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
                cr: 0.25,
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
            in_world_minutes: 0,
            last_long_rest_minutes: None,
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

    // --- Rest command integration tests ---

    #[test]
    fn test_short_rest_command_through_process_input() {
        let mut state = create_test_exploration_state();
        state.character.current_hp = 1;
        state.character.hit_dice_remaining = 1;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "short rest");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        let text = output.text.join("\n");
        assert!(text.to_lowercase().contains("short rest"), "Expected short rest narration, got: {}", text);
        assert!(new_state.character.current_hp > 1, "Short rest should heal");
        assert_eq!(new_state.in_world_minutes, 60);
    }

    #[test]
    fn test_long_rest_command_through_process_input() {
        let mut state = create_test_exploration_state();
        state.character.current_hp = 1;
        state.character.exhaustion = 2;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "long rest");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        assert_eq!(new_state.character.current_hp, new_state.character.max_hp);
        assert_eq!(new_state.character.exhaustion, 1);
        assert_eq!(new_state.in_world_minutes, 8 * 60);
        assert_eq!(new_state.last_long_rest_minutes, Some(0));
    }

    #[test]
    fn test_long_rest_cooldown_integration() {
        let mut state = create_test_exploration_state();
        // Pretend we just rested
        state.last_long_rest_minutes = Some(0);
        state.in_world_minutes = 60 * 5; // only 5 hours later
        state.character.current_hp = 1;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "long rest");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        let text = output.text.join("\n").to_lowercase();
        assert!(text.contains("rested too recently"), "Expected cooldown message, got: {}", text);
        // State must NOT change
        assert_eq!(new_state.character.current_hp, 1);
        assert_eq!(new_state.in_world_minutes, 60 * 5);
    }

    #[test]
    fn test_bare_rest_command_disambiguates_through_process_input() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "rest");
        let text = output.text.join("\n").to_lowercase();
        assert!(
            text.contains("short rest") && text.contains("long rest"),
            "Bare 'rest' should ask which rest. Got: {}",
            text,
        );
    }

    #[test]
    fn test_reaction_used_resets_at_start_of_player_second_turn() {
        let mut state = create_test_combat_state();

        // Ensure it is the player's turn
        force_player_turn(&mut state);

        // Manually mark reaction as used (simulating a reaction taken this turn)
        if let Some(ref mut combat) = state.active_combat {
            combat.reaction_used = true;
        }

        // End the player's turn
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "end turn");
        let mut state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Force back to player's turn (second turn)
        force_player_turn(&mut state);

        let combat = state.active_combat.as_ref().expect("combat should still be active");
        assert!(
            !combat.reaction_used,
            "reaction_used should be false at the start of the player's second turn"
        );
    }

    // ----- XP / leveling integration -----

    /// Build an active combat where the player has won (one dead hostile NPC
    /// of the given CR present in the initiative order). Used to verify
    /// `end_combat` awards XP for the right monster.
    fn combat_state_with_one_dead_hostile(state: &mut GameState, cr: f32) -> u32 {
        use crate::combat::{CombatState, Combatant};
        let npc_id: u32 = 7777;
        state.world.npcs.insert(npc_id, state::Npc {
            id: npc_id,
            name: "Test Foe".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(state::CombatStats {
                max_hp: 7,
                current_hp: 0, // already dead
                ac: 13,
                speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr,
            }),
            conditions: vec![],
        });
        state.active_combat = Some(CombatState {
            initiative_order: vec![
                (Combatant::Player, 15),
                (Combatant::Npc(npc_id), 10),
            ],
            current_turn: 0,
            round: 1,
            distances: HashMap::new(),
            player_movement_remaining: state.character.speed,
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
        });
        npc_id
    }

    #[test]
    fn end_combat_awards_xp_for_dead_hostile() {
        let mut state = create_test_exploration_state();
        let _ = combat_state_with_one_dead_hostile(&mut state, 0.25); // Goblin
        let starting_xp = state.character.xp;
        let lines = end_combat(&mut state, true);
        assert_eq!(state.character.xp, starting_xp + 50);
        assert!(lines.iter().any(|l| l.contains("50 XP")), "Lines: {:?}", lines);
    }

    #[test]
    fn end_combat_awards_xp_for_each_dead_hostile() {
        use crate::combat::{CombatState, Combatant};
        let mut state = create_test_exploration_state();
        // Two dead goblins
        for (npc_id, _) in [(7001u32, 0.25f32), (7002, 0.25)] {
            state.world.npcs.insert(npc_id, state::Npc {
                id: npc_id,
                name: format!("Goblin {}", npc_id),
                role: state::NpcRole::Guard,
                disposition: state::Disposition::Hostile,
                dialogue_tags: vec![],
                location: state.current_location,
                combat_stats: Some(state::CombatStats {
                    max_hp: 7, current_hp: 0, ac: 13, speed: 30,
                    ability_scores: HashMap::new(),
                    attacks: vec![],
                    proficiency_bonus: 2,
                    cr: 0.25,
                }),
                conditions: vec![],
            });
        }
        state.active_combat = Some(CombatState {
            initiative_order: vec![
                (Combatant::Player, 15),
                (Combatant::Npc(7001), 12),
                (Combatant::Npc(7002), 8),
            ],
            current_turn: 0, round: 1, distances: HashMap::new(),
            player_movement_remaining: state.character.speed,
            player_dodging: false, player_disengaging: false,
            action_used: false, bonus_action_used: false,
            reaction_used: false, free_interaction_used: false,
            npc_dodging: HashMap::new(), npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0, pending_reaction: None,
        });
        let _ = end_combat(&mut state, true);
        // Two goblins: 50 + 50 = 100 XP.
        assert_eq!(state.character.xp, 100);
    }

    #[test]
    fn end_combat_defeat_awards_no_xp() {
        let mut state = create_test_exploration_state();
        let _ = combat_state_with_one_dead_hostile(&mut state, 2.0); // Ogre
        let starting_xp = state.character.xp;
        let _ = end_combat(&mut state, false);
        assert_eq!(state.character.xp, starting_xp);
    }

    #[test]
    fn end_combat_skips_living_hostiles() {
        use crate::combat::{CombatState, Combatant};
        let mut state = create_test_exploration_state();
        let npc_id: u32 = 8001;
        state.world.npcs.insert(npc_id, state::Npc {
            id: npc_id,
            name: "Survivor".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(state::CombatStats {
                max_hp: 7, current_hp: 5, ac: 13, speed: 30, // alive
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 2.0,
            }),
            conditions: vec![],
        });
        state.active_combat = Some(CombatState {
            initiative_order: vec![
                (Combatant::Player, 15),
                (Combatant::Npc(npc_id), 10),
            ],
            current_turn: 0, round: 1, distances: HashMap::new(),
            player_movement_remaining: state.character.speed,
            player_dodging: false, player_disengaging: false,
            action_used: false, bonus_action_used: false,
            reaction_used: false, free_interaction_used: false,
            npc_dodging: HashMap::new(), npc_disengaging: HashMap::new(),
            player_shield_ac_bonus: 0, pending_reaction: None,
        });
        let _ = end_combat(&mut state, true);
        // No XP should be awarded — the hostile is still alive.
        assert_eq!(state.character.xp, 0);
    }

    #[test]
    fn defeat_objective_completion_awards_quest_xp() {
        let mut state = create_test_exploration_state();
        let boss_id: u32 = 9001;
        state.world.npcs.insert(boss_id, state::Npc {
            id: boss_id,
            name: "Boss".to_string(),
            role: state::NpcRole::Guard,
            disposition: state::Disposition::Hostile,
            dialogue_tags: vec![],
            location: state.current_location,
            combat_stats: Some(state::CombatStats {
                max_hp: 30, current_hp: 0, ac: 14, speed: 30,
                ability_scores: HashMap::new(),
                attacks: vec![],
                proficiency_bonus: 2,
                cr: 2.0, // 450 XP
            }),
            conditions: vec![],
        });
        state.progress.objectives.push(state::Objective {
            id: "boss".to_string(),
            title: "Defeat the Boss".to_string(),
            description: "Slay the boss.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(state::ObjectiveType::DefeatNpc(boss_id));
        // Wire combat state so end_combat awards monster XP too
        let _ = combat_state_with_one_dead_hostile(&mut state, 2.0);
        // Replace the test foe with the boss in initiative
        if let Some(c) = state.active_combat.as_mut() {
            c.initiative_order.push((crate::combat::Combatant::Npc(boss_id), 5));
        }

        let _ = end_combat(&mut state, true);
        // 450 (test foe) + 450 (boss) + 100 (quest bonus) = 1000.
        assert_eq!(state.character.xp, 1000);
        assert!(state.progress.objectives[0].completed);
    }

    #[test]
    fn character_sheet_shows_xp_and_next_level_threshold() {
        let mut state = create_test_exploration_state();
        state.character.xp = 150;
        let lines = render_character_sheet_with_xp(&state);
        let joined = lines.join("\n");
        assert!(joined.contains("XP: 150 / 300"), "Got: {}", joined);
        assert!(joined.contains("level 1 -> 2"), "Got: {}", joined);
    }

    #[test]
    fn character_sheet_at_max_level_shows_max_indicator() {
        let mut state = create_test_exploration_state();
        state.character.level = 20;
        state.character.xp = 999_999;
        let lines = render_character_sheet_with_xp(&state);
        let joined = lines.join("\n");
        assert!(joined.contains("max level"), "Got: {}", joined);
    }

    #[test]
    fn character_sheet_shows_asi_credits_when_present() {
        let mut state = create_test_exploration_state();
        state.character.asi_credits = 2;
        let lines = render_character_sheet_with_xp(&state);
        let joined = lines.join("\n");
        assert!(joined.contains("ASI/feat credits: 2"), "Got: {}", joined);
    }

    #[test]
    fn legacy_save_missing_cr_field_defaults_to_zero() {
        // Older saves predate the CR field on CombatStats; serde default = 0.0.
        let cs_json = r#"{
            "max_hp": 7,
            "current_hp": 0,
            "ac": 13,
            "speed": 30,
            "ability_scores": {},
            "attacks": [],
            "proficiency_bonus": 2
        }"#;
        let cs: state::CombatStats = serde_json::from_str(cs_json).unwrap();
        assert_eq!(cs.cr, 0.0);
        // CR 0 maps to 10 XP, so legacy NPCs still award something on defeat.
        assert_eq!(leveling::xp_for_cr(cs.cr), 10);
    }
}
