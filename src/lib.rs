pub mod types;
pub mod rules;
pub mod character;
pub mod state;
pub mod parser;
pub mod world;
pub mod narration;
pub mod output;

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

pub fn new_game(seed: u64) -> GameOutput {
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

    let result = match state.game_phase {
        GamePhase::CharacterCreation(step) => handle_creation(&mut state, input, step),
        GamePhase::Exploration => handle_exploration(&mut state, input),
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

fn npc_candidates(state: &GameState) -> Vec<(usize, String)> {
    let loc = match state.world.locations.get(&state.current_location) {
        Some(loc) => loc,
        None => return Vec::new(),
    };
    loc.npcs.iter()
        .filter_map(|&id| state.world.npcs.get(&id).map(|npc| (id as usize, npc.name.clone())))
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
                        let id = id as u32;
                        if let Some(npc) = state.world.npcs.get(&id) {
                            vec![format!("{} — {} ({})", npc.name, npc.role_description(), npc.disposition_description())]
                        } else if let Some(item) = state.world.items.get(&id) {
                            vec![format!("{}: {}", item.name, item.description)]
                        } else {
                            vec![format!("You don't see any \"{}\" here.", target)]
                        }
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
                    let id = id as u32;
                    if let Some(npc) = state.world.npcs.get(&id) {
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
                    if let Some(item) = state.world.items.get_mut(&item_id) {
                        item.carried_by_player = false;
                        item.location = Some(current_location);
                    }
                    if let Some(loc) = state.world.locations.get_mut(&current_location) {
                        loc.items.push(item_id);
                    }
                    let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| item_name.clone());
                    vec![format!("You drop the {}.", name)]
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![format!("You don't have any \"{}\".", item_name)],
            }
        }
        Command::Use(item_name) => {
            let owned_candidates = inventory_item_candidates(state);
            let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                .map(|(id, name)| (*id, name.as_str()))
                .collect();

            match resolver::resolve_target(&item_name, &candidates) {
                ResolveResult::Found(id) => {
                    let item_id = id as u32;
                    let name = state.world.items.get(&item_id).map(|i| i.name.clone()).unwrap_or_else(|| item_name.clone());
                    vec![format!("You use the {}. (Effects not yet implemented.)", name)]
                }
                ResolveResult::Ambiguous(matches) => resolver::format_disambiguation(&matches),
                ResolveResult::NotFound => vec![format!("You don't have any \"{}\".", item_name)],
            }
        }
        Command::Inventory => {
            if state.character.inventory.is_empty() {
                return vec![narration::templates::EMPTY_INVENTORY.to_string()];
            }
            let mut lines = vec!["You are carrying:".to_string()];
            for &item_id in &state.character.inventory {
                if let Some(item) = state.world.items.get(&item_id) {
                    lines.push(format!("  - {}", item.name));
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
        Command::Help(_) => {
            vec![narration::templates::HELP_TEXT.to_string()]
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
        let output = new_game(42);
        assert!(output.text.iter().any(|t| t.contains("Choose your race")));
    }

    #[test]
    fn test_full_character_creation_flow() {
        let output = new_game(42);
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
        }
    }
}
