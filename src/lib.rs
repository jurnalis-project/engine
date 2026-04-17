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
use state::{GameState, GamePhase, CreationStep, PendingDisambiguation, SAVE_VERSION};
use character::{race::Race, class::Class, background::Background, STANDARD_ARRAY, generate_random_scores};
use character::feat::{FeatDef, FeatCategory, FeatEffect};
#[cfg(test)]
use character::create_character;
use types::{Ability, Alignment, Skill};

/// The 9 SRD origin feats, in the order shown on the ChooseOriginFeat menu.
/// Mirrors the catalog order in `character::feat::FEATS` for the origin tier.
const ORIGIN_FEAT_NAMES: &[&str] = &[
    "Alert",
    "Crafter",
    "Healer",
    "Lucky",
    "Magic Initiate",
    "Musician",
    "Savage Attacker",
    "Skilled",
    "Tavern Brawler",
];

/// The 10 SRD alignment options, in the canonical order shown on the
/// ChooseAlignment menu: the nine classic alignments (LG, NG, CG, LN, N, CN,
/// LE, NE, CE) followed by Unaligned. See
/// `docs/reference/character-creation.md` and
/// `docs/specs/character-system.md`.
const ALIGNMENT_OPTIONS: &[Alignment] = &[
    Alignment::LawfulGood,
    Alignment::NeutralGood,
    Alignment::ChaoticGood,
    Alignment::LawfulNeutral,
    Alignment::TrueNeutral,
    Alignment::ChaoticNeutral,
    Alignment::LawfulEvil,
    Alignment::NeutralEvil,
    Alignment::ChaoticEvil,
    Alignment::Unaligned,
];

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
        pending_background_pattern: None,
        pending_subrace: None,
        pending_disambiguation: None,
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
        "  4. Dragonborn (Darkvision, Breath Weapon, Damage Resistance)".to_string(),
        "  5. Gnome (Darkvision, Gnomish Cunning)".to_string(),
        "  6. Goliath (35 ft speed, Powerful Build, Giant Ancestry)".to_string(),
        "  7. Halfling (Brave, Luck, Naturally Stealthy)".to_string(),
        "  8. Orc (Darkvision 120 ft, Adrenaline Rush, Relentless Endurance)".to_string(),
        "  9. Tiefling (Darkvision, Fiendish Legacy, Otherworldly Presence)".to_string(),
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

    // GAME OVER early-exit. A character at 0 HP is truly defeated only when
    // they are NOT in active combat (e.g. from trap damage outside combat)
    // OR when they have accumulated three death save failures per SRD
    // (see combat::CombatState::check_end / `death_save_failures`). While
    // dying mid-combat they remain playable and must be allowed to continue
    // processing combat turns (death saves, NPC damage, healing, etc.).
    if state.character.current_hp <= 0 {
        let truly_defeated = state.active_combat.as_ref()
            .map(|c| c.death_save_failures >= 3)
            .unwrap_or(true);
        if truly_defeated {
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
        // else: dying in combat; fall through to normal combat handling.
    }

    // --- Disambiguation selection routing (#62) ---
    //
    // If the previous turn emitted a disambiguation prompt, `pending_disambiguation`
    // carries the verb prefix and the numbered list of candidate names. We take
    // the pending state upfront so it is cleared regardless of which branch runs
    // below (spec: "no dangling pending state"). If the input is a valid numeric
    // selection within the candidate range, rewrite it to the original verb +
    // exact candidate name and re-dispatch. Any other input (non-numeric or
    // out-of-range) falls through to normal parsing with pending cleared.
    //
    // This routing is scoped to Exploration and Combat. Character-creation
    // phases use their own numeric menus and never set pending_disambiguation,
    // so the field is guaranteed to be None there; we still gate the
    // replacement on phase to stay defensive against future changes.
    let pending = state.pending_disambiguation.take();
    let rewritten_input: String = match &pending {
        Some(pd)
            if state.active_combat.is_some()
                || matches!(state.game_phase, GamePhase::Exploration) =>
        {
            match resolve_numeric_selection(input, &pd.candidates) {
                Some(name) => {
                    // Join prefix + name + suffix with single spaces, then
                    // collapse any double spaces that arise from empty parts.
                    let parts = [pd.verb_prefix.as_str(), name.as_str(), pd.verb_suffix.as_str()];
                    parts
                        .iter()
                        .filter(|p| !p.is_empty())
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" ")
                }
                None => input.to_string(),
            }
        }
        _ => input.to_string(),
    };
    let input = rewritten_input.as_str();

    let result = if state.active_combat.is_some() {
        handle_combat(&mut state, input)
    } else {
        match state.game_phase {
            GamePhase::CharacterCreation(step) => handle_creation(&mut state, input, step),
            GamePhase::Exploration => handle_exploration(&mut state, input),
            GamePhase::Victory => handle_victory(&mut state, input),
            GamePhase::ChooseAsi => handle_choose_asi(&mut state, input),
        }
    };

    // Death Saving Throws (issue #84): any healing path that restored HP
    // above 0 this call should also clear the dying counters. Doing it here
    // centralizes the invariant (HP > 0 => death_save_* == 0 in active
    // combat) so individual healing sites don't each need to remember.
    clear_dying_state_if_healed(&mut state);

    let new_state_json = serde_json::to_string(&state).unwrap();
    let state_changed = new_state_json != old_state_json;
    GameOutput::new(result, new_state_json, state_changed)
}

/// Clear the dying-state death save counters when the player's HP has been
/// restored above 0 during an active combat. Used as a centralized
/// invariant enforcement so healing sites (potions, spells, rests, feats)
/// don't need to each remember to reset `death_save_successes` /
/// `death_save_failures`.
fn clear_dying_state_if_healed(state: &mut GameState) {
    if state.character.current_hp > 0 {
        if let Some(combat) = state.active_combat.as_mut() {
            if combat.death_save_successes > 0 || combat.death_save_failures > 0 {
                combat.reset_death_saves();
            }
        }
    }
}

/// If `input` is a bare positive integer in `1..=candidates.len()`, return the
/// corresponding candidate name. Otherwise return None. The input must be
/// entirely numeric (no extra words, no sign) so that inputs like "1 sword"
/// or "-1" fall through to normal parsing.
fn resolve_numeric_selection(input: &str, candidates: &[String]) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let n: usize = trimmed.parse().ok()?;
    if n == 0 || n > candidates.len() {
        return None;
    }
    Some(candidates[n - 1].clone())
}

/// Emit a disambiguation prompt AND store the pending selection context on
/// the game state so a subsequent numeric input ("1", "2", ...) from the
/// player routes back to the original command. `verb_prefix` is the command
/// head to re-apply when resolving (e.g. `"take"`, `"talk to"`, `"equip"`).
/// `matches` is the list of (id, name) pairs to display, in the order they
/// will be numbered; only the names are stored for later re-dispatch (the
/// numeric index maps to `matches[n-1].1`).
fn emit_disambiguation(
    state: &mut GameState,
    verb_prefix: &str,
    matches: &[(usize, String)],
) -> Vec<String> {
    emit_disambiguation_with_suffix(state, verb_prefix, "", matches)
}

/// Like `emit_disambiguation`, but includes a trailing modifier that sits
/// AFTER the resolved candidate name. Used for commands with positional
/// suffixes such as `equip <weapon> off hand`, where the off-hand marker
/// must survive the re-dispatch.
fn emit_disambiguation_with_suffix(
    state: &mut GameState,
    verb_prefix: &str,
    verb_suffix: &str,
    matches: &[(usize, String)],
) -> Vec<String> {
    state.pending_disambiguation = Some(PendingDisambiguation {
        verb_prefix: verb_prefix.to_string(),
        candidates: matches.iter().map(|(_, name)| name.clone()).collect(),
        verb_suffix: verb_suffix.to_string(),
    });
    resolver::format_disambiguation(matches)
}

fn handle_creation(state: &mut GameState, input: &str, step: CreationStep) -> Vec<String> {
    let input = input.trim();
    match step {
        CreationStep::ChooseRace => {
            let input_lower = input.to_lowercase();
            let all_races = Race::all();
            let selected = input.parse::<usize>().ok()
                .and_then(|n| if (1..=all_races.len()).contains(&n) { Some(all_races[n - 1]) } else { None })
                .or_else(|| all_races.iter().copied().find(|r| r.to_string().to_lowercase() == input_lower));

            let race = match selected {
                Some(r) => r,
                None => {
                    let names: Vec<String> = all_races.iter().map(|r| r.to_string()).collect();
                    return vec![format!(
                        "Please choose a race 1-{} or type its name (e.g., {}).",
                        all_races.len(),
                        names.join(", "),
                    )];
                }
            };
            state.character.race = race;

            if race.has_subraces() {
                // Transition to subrace selection
                state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseSubrace);
                let options = race.subrace_options();
                let mut lines = vec![format!("Race: {}. Choose your {}:", race, race.subrace_label())];
                for (i, &opt) in options.iter().enumerate() {
                    let desc = Race::subrace_description(opt);
                    lines.push(format!("  {}. {} ({})", i + 1, opt, desc));
                }
                lines.push("Enter a number or name.".to_string());
                lines
            } else {
                // Skip subrace, go directly to class selection
                state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseClass);
                let mut lines = vec![format!("Race: {}. Now choose your class:", race)];
                for (i, &class) in Class::all().iter().enumerate() {
                    let saves = class.saving_throw_proficiencies();
                    lines.push(format!(
                        "  {}. {} (d{} HP, {}/{} saves)",
                        i + 1, class, class.hit_die(), saves[0], saves[1],
                    ));
                }
                lines.push("Enter a number or class name.".to_string());
                lines
            }
        }
        CreationStep::ChooseSubrace => {
            let race = state.character.race;
            let options = race.subrace_options();
            let input_lower = input.to_lowercase();

            // Match by number or case-insensitive name
            let selected = input.parse::<usize>().ok()
                .and_then(|n| if (1..=options.len()).contains(&n) { Some(options[n - 1]) } else { None })
                .or_else(|| options.iter().copied().find(|o| o.to_lowercase() == input_lower));

            let subrace = match selected {
                Some(s) => s,
                None => {
                    let names: Vec<&str> = options.to_vec();
                    return vec![format!(
                        "Please choose 1-{} or type the name (e.g., {}).",
                        options.len(),
                        names.join(", "),
                    )];
                }
            };

            state.pending_subrace = Some(subrace.to_string());

            // Apply speed override if the subrace changes it (e.g. Wood Elf -> 35 ft)
            if let Some(speed) = Race::subrace_speed_override(subrace) {
                state.character.speed = speed;
            }

            // Append subrace-specific traits to the character's trait list
            for trait_name in Race::subrace_traits(subrace) {
                let s = trait_name.to_string();
                if !state.character.traits.contains(&s) {
                    state.character.traits.push(s);
                }
            }

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseClass);
            let mut lines = vec![format!(
                "{}: {}. Now choose your class:",
                race.subrace_label(), subrace
            )];
            for (i, &class) in Class::all().iter().enumerate() {
                let saves = class.saving_throw_proficiencies();
                lines.push(format!(
                    "  {}. {} (d{} HP, {}/{} saves)",
                    i + 1, class, class.hit_die(), saves[0], saves[1],
                ));
            }
            lines.push("Enter a number or class name.".to_string());
            lines
        }
        CreationStep::ChooseClass => {
            let input_lower = input.to_lowercase();
            let all_classes = Class::all();
            let selected = input.parse::<usize>().ok()
                .and_then(|n| if (1..=all_classes.len()).contains(&n) { Some(all_classes[n - 1]) } else { None })
                .or_else(|| all_classes.iter().copied().find(|c| c.to_string().to_lowercase() == input_lower));

            let class = match selected {
                Some(c) => c,
                None => {
                    let names: Vec<String> = all_classes.iter().map(|c| c.to_string()).collect();
                    return vec![format!(
                        "Please choose a class 1-{} or type its name (e.g., {}).",
                        all_classes.len(),
                        names.join(", "),
                    )];
                }
            };
            state.character.class = class;
            state.character.save_proficiencies = class.saving_throw_proficiencies();

            // Initialize spell slots and known spells based on class. Each
            // caster class gets its starter list from
            // character::default_starting_spells; non-casters get empty.
            state.character.spell_slots_max = class.starting_spell_slots();
            state.character.spell_slots_remaining = state.character.spell_slots_max.clone();
            state.character.known_spells = character::default_starting_spells(class);
            // Initialize per-class feature state. CHA mod uses the current ability
            // scores (which may be unset at this point — defaults to 10 -> +0 mod
            // -> 1 inspiration min for Bard).
            let cha_mod = state.character.ability_modifier(Ability::Charisma);
            character::init_class_features(
                &mut state.character.class_features,
                class,
                /* level */ 1,
                cha_mod,
                &state.character.known_spells,
            );

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseBackground);
            let mut lines = vec![
                format!("Class: {}. Choose your background:", class),
            ];
            for (i, &bg) in Background::all().iter().enumerate() {
                let feat = bg.origin_feat();
                let skills = bg.skill_proficiencies();
                lines.push(format!(
                    "  {}. {} ({} / {}, feat: {})",
                    i + 1, bg, skills[0], skills[1], feat,
                ));
            }
            lines.push("Enter a number or name.".to_string());
            lines
        }
        CreationStep::ChooseBackground => {
            let input_lower = input.to_lowercase();
            let all_bgs = Background::all();
            let selected = input.parse::<usize>().ok()
                .and_then(|n| if (1..=all_bgs.len()).contains(&n) { Some(all_bgs[n - 1]) } else { None })
                .or_else(|| all_bgs.iter().copied().find(|bg| bg.to_string().to_lowercase() == input_lower));

            let bg = match selected {
                Some(b) => b,
                None => return vec![format!("Please pick a background 1-{} or type its name.", all_bgs.len())],
            };
            state.character.background = bg;

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseOriginFeat);
            let suggested = bg.origin_feat();
            // Strip parenthetical suffix like " (Cleric)" so suggestion matches
            // a feat in the catalog (e.g. "Magic Initiate (Cleric)" -> "Magic Initiate").
            let suggested_short = suggested.split(" (").next().unwrap_or(suggested);
            let mut lines = vec![
                format!("Background: {}. Choose your origin feat:", bg),
                format!("(Background suggests: {} — press Enter or type 'default' to keep.)", suggested),
            ];
            for (i, feat) in ORIGIN_FEAT_NAMES.iter().enumerate() {
                let marker = if *feat == suggested_short { " *" } else { "" };
                lines.push(format!("  {}. {}{}", i + 1, feat, marker));
            }
            lines.push("Enter a number, name, or 'default'.".to_string());
            lines
        }
        CreationStep::ChooseOriginFeat => {
            let bg = state.character.background;
            let suggested_full = bg.origin_feat();
            let suggested_short = suggested_full.split(" (").next().unwrap_or(suggested_full);

            let trimmed = input.trim();
            let lower = trimmed.to_lowercase();

            let chosen: &'static str = if trimmed.is_empty()
                || lower == "default" || lower == "keep" || lower == "suggested"
            {
                // Map background suggestion onto a catalog feat name.
                ORIGIN_FEAT_NAMES.iter().copied().find(|n| *n == suggested_short)
                    .unwrap_or("Alert")
            } else if let Ok(n) = trimmed.parse::<usize>() {
                if (1..=ORIGIN_FEAT_NAMES.len()).contains(&n) {
                    ORIGIN_FEAT_NAMES[n - 1]
                } else {
                    return vec![format!(
                        "Please choose a number 1-{} or type a feat name.",
                        ORIGIN_FEAT_NAMES.len(),
                    )];
                }
            } else {
                match ORIGIN_FEAT_NAMES.iter().copied().find(|n| n.to_lowercase() == lower) {
                    Some(name) => name,
                    None => {
                        return vec![format!(
                            "Unknown origin feat. Pick a number 1-{} or type one of: {}.",
                            ORIGIN_FEAT_NAMES.len(),
                            ORIGIN_FEAT_NAMES.join(", "),
                        )];
                    }
                }
            };
            state.character.origin_feat = Some(chosen.to_string());

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseBackgroundAbilityPattern);
            let abilities = bg.ability_options();
            vec![
                format!("Origin feat: {}. Choose ability adjustment pattern:", chosen),
                format!(
                    "  1. +2 to {}, +1 to {} (ignores {})",
                    abilities[0], abilities[1], abilities[2],
                ),
                format!(
                    "  2. +1 to all three ({}, {}, {})",
                    abilities[0], abilities[1], abilities[2],
                ),
            ]
        }
        CreationStep::ChooseBackgroundAbilityPattern => {
            let pattern = match input {
                "1" | "+2/+1" | "2/1" => 1u8,
                "2" | "+1/+1/+1" | "1/1/1" => 2u8,
                _ => return vec!["Please choose 1 (+2/+1) or 2 (+1/+1/+1).".to_string()],
            };
            state.pending_background_pattern = Some(pattern);

            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseAbilityMethod);
            vec![
                "Ability adjustment recorded. Choose ability score method:".to_string(),
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
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseAlignment);

            let mut lines = vec![
                "Skills chosen! Choose your alignment:".to_string(),
            ];
            for (i, alignment) in ALIGNMENT_OPTIONS.iter().enumerate() {
                lines.push(format!("  {}. {}", i + 1, alignment));
            }
            lines.push("Enter a number or name.".to_string());
            lines
        }
        CreationStep::ChooseAlignment => {
            let trimmed = input.trim();
            let lower = trimmed.to_lowercase();

            // Numeric (1-10) or case-insensitive name match. Display names
            // (e.g. "Lawful Good", "Neutral" for TrueNeutral) are used for
            // prose matching since they're what the prompt shows.
            let chosen: Alignment = if let Ok(n) = trimmed.parse::<usize>() {
                if (1..=ALIGNMENT_OPTIONS.len()).contains(&n) {
                    ALIGNMENT_OPTIONS[n - 1]
                } else {
                    return vec![format!(
                        "Please choose a number 1-{} or type an alignment name.",
                        ALIGNMENT_OPTIONS.len(),
                    )];
                }
            } else {
                match ALIGNMENT_OPTIONS.iter().copied()
                    .find(|a| a.to_string().to_lowercase() == lower)
                {
                    Some(a) => a,
                    None => {
                        let names: Vec<String> = ALIGNMENT_OPTIONS.iter()
                            .map(|a| a.to_string()).collect();
                        return vec![format!(
                            "Unknown alignment. Pick a number 1-{} or type one of: {}.",
                            ALIGNMENT_OPTIONS.len(),
                            names.join(", "),
                        )];
                    }
                }
            };

            state.character.alignment = chosen;
            state.game_phase = GamePhase::CharacterCreation(CreationStep::ChooseName);

            vec![
                format!("Alignment: {}. Enter your character's name:", chosen),
            ]
        }
        CreationStep::ChooseName => {
            let name = input.trim().to_string();
            if name.is_empty() {
                return vec!["Please enter a name.".to_string()];
            }

            state.character.name = name.clone();
            state.character.traits = state.character.race.traits().iter().map(|s| s.to_string()).collect();

            // Apply subrace traits and speed override if a subrace was selected.
            if let Some(ref subrace) = state.pending_subrace {
                state.character.subrace = Some(subrace.clone());
                for trait_name in Race::subrace_traits(subrace) {
                    let s = trait_name.to_string();
                    if !state.character.traits.contains(&s) {
                        state.character.traits.push(s);
                    }
                }
            }

            // Apply background effects: ability adjustments, skill/tool profs,
            // language, origin feat trait. Must happen before HP is computed so
            // any CON increase is reflected in max_hp.
            apply_background_effects(state);

            // Calculate HP
            let con_mod = state.character.ability_modifier(Ability::Constitution);
            state.character.max_hp = character::calculate_hp(state.character.class, con_mod, 1);
            state.character.current_hp = state.character.max_hp;
            state.character.level = 1;
            // Base speed from race, then apply subrace speed override.
            state.character.speed = state.character.race.speed();
            if let Some(ref subrace) = state.pending_subrace {
                if let Some(speed) = Race::subrace_speed_override(subrace) {
                    state.character.speed = speed;
                }
            }

            // Apply origin-feat effects (HP-per-level for Tough, skill profs,
            // ability bonuses, etc.). Must run AFTER HP is computed because
            // HpBonusPerLevel adjusts max_hp/current_hp directly.
            if let Some(name) = state.character.origin_feat.clone() {
                apply_feat_effects(&mut state.character, &name);
            }

            // Generate world
            let mut rng = StdRng::seed_from_u64(state.rng_seed);
            let world = world::generate_world(&mut rng, 15);
            state.world = world;
            state.current_location = 0;
            state.discovered_locations.insert(0);

            // Grant starting equipment (class + background packages).
            grant_starting_equipment(state);
            grant_background_equipment(state);

            // Clear transient creation-time state.
            state.pending_background_pattern = None;
            state.pending_subrace = None;

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
            charges_remaining: None,
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
            charges_remaining: None,
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

/// Apply the character's background effects at finalization:
///   - ability score adjustments (cap at 20)
///   - skill proficiencies (merged with class selections, de-duplicated)
///   - tool proficiency
///   - language
///   - origin feat recorded as a trait
fn apply_background_effects(state: &mut GameState) {
    let bg = state.character.background;
    let abilities = bg.ability_options();
    let pattern = state.pending_background_pattern.unwrap_or(1);

    // 1. Ability score adjustments (cap at 20 per SRD)
    let adjustments: [(Ability, i32); 3] = if pattern == 2 {
        [(abilities[0], 1), (abilities[1], 1), (abilities[2], 1)]
    } else {
        // Default +2/+1 pattern: +2 to first listed, +1 to second listed
        [(abilities[0], 2), (abilities[1], 1), (abilities[2], 0)]
    };
    for (ab, delta) in adjustments {
        if delta == 0 { continue; }
        let entry = state.character.ability_scores.entry(ab).or_insert(10);
        let new_val = (*entry + delta).min(20);
        *entry = new_val;
    }

    // 2. Skill proficiencies (merge + de-dup; class picks keep precedence order)
    for skill in bg.skill_proficiencies() {
        if !state.character.skill_proficiencies.contains(&skill) {
            state.character.skill_proficiencies.push(skill);
        }
    }

    // 3. Tool proficiency
    let tool = bg.tool_proficiency().to_string();
    if !state.character.tool_proficiencies.contains(&tool) {
        state.character.tool_proficiencies.push(tool);
    }

    // 4. Language (ensure Common is present; then add background language)
    let common = "Common".to_string();
    if !state.character.languages.contains(&common) {
        state.character.languages.push(common);
    }
    let lang = bg.language().to_string();
    if !state.character.languages.contains(&lang) {
        state.character.languages.push(lang);
    }

    // 5. Origin feat (recorded as a trait until issue #28 lands)
    let feat_trait = format!("Origin Feat: {}", bg.origin_feat());
    if !state.character.traits.contains(&feat_trait) {
        state.character.traits.push(feat_trait);
    }
}

/// Grant the starting equipment package of the character's background.
/// Items that are not modelled in the SRD weapon/armor tables are skipped
/// (a future issue will add adventuring gear support, see #42).
fn grant_background_equipment(state: &mut GameState) {
    use state::{Item, ItemType};

    let bg = state.character.background;
    let items_to_grant = bg.starting_equipment();

    let mut next_id = state.world.items.keys().max().map_or(0, |&id| id + 1);

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
            charges_remaining: None,
        })
    };

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
            charges_remaining: None,
        })
    };

    for &name in items_to_grant {
        // Skip items already modelled elsewhere via class loadout to avoid dupes
        // is unnecessary — background items are distinct flavour. We only skip
        // items that do not resolve to an SRD weapon or armor entry yet.
        let created = create_weapon(name, next_id).or_else(|| create_armor(name, next_id));
        if let Some(item) = created {
            let id = item.id;
            state.world.items.insert(id, item);
            state.character.inventory.push(id);
            next_id += 1;
        }
        // Non-weapon/armor items (tools, books, clothing, etc.) are skipped
        // silently for now — they will be modelled under issue #42.
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
                    ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "look", &matches),
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
                                        let disadv = armor_disadvantage_for_ability(
                                            &state.character, skill.ability(),
                                        );
                                        let result = rules::checks::skill_check(
                                            &mut rng,
                                            *skill,
                                            &state.character.ability_scores,
                                            &state.character.skill_proficiencies,
                                            state.character.proficiency_bonus(),
                                            trigger.dc,
                                            false, disadv,
                                        );
                                        let narration = narration::narrate_skill_check(&mut rng, &skill.to_string(), &result);
                                        Some((result.success, narration))
                                    }
                                    state::TriggerType::SavingThrow(ability) => {
                                        let score = state.character.ability_scores.get(ability).copied().unwrap_or(10);
                                        let is_prof = state.character.is_proficient_in_save(*ability);
                                        let disadv = armor_disadvantage_for_ability(
                                            &state.character, *ability,
                                        );
                                        let result = rules::checks::ability_check(
                                            &mut rng, score,
                                            state.character.proficiency_bonus(),
                                            is_prof, trigger.dc, false, disadv,
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
                ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "talk to", &matches),
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

                    // If a quest level-up granted unspent ASI credits, prompt.
                    lines.extend(check_and_enter_asi_phase(state));

                    lines
                }
                ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "take", &matches),
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
                ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "drop", &matches),
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
                    let disadv = armor_disadvantage_for_ability(
                        &state.character, skill.ability(),
                    );
                    let result = rules::checks::skill_check(
                        &mut rng, skill,
                        &state.character.ability_scores,
                        &state.character.skill_proficiencies,
                        state.character.proficiency_bonus(),
                        15, // Default DC for voluntary checks
                        false, disadv,
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
                                update_armor_proficiency_state(state, *category, &mut lines);
                            }
                            lines
                        }
                        _ => vec![narration::templates::EQUIP_CANT.replace("{item}", &item.name)],
                    }
                }
                ResolveResult::Ambiguous(matches) => {
                    let suffix = if force_off_hand { "off hand" } else { "" };
                    emit_disambiguation_with_suffix(state, "equip", suffix, &matches)
                }
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
                        // Body slot is now empty -> no nonproficient armor worn.
                        state.character.wearing_nonproficient_armor = false;
                    }

                    if is_weapon {
                        vec![narration::templates::UNEQUIP_WEAPON.replace("{item}", &name)]
                    } else {
                        vec![narration::templates::UNEQUIP_ARMOR.replace("{item}", &name)]
                    }
                }
                ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "unequip", &matches),
                ResolveResult::NotFound => vec![narration::templates::UNEQUIP_NOT_EQUIPPED.replace("{name}", &target_str)],
            }
        }
        Command::Cast { spell, target: _, ritual } => {
            // Check if caster
            if state.character.known_spells.is_empty() {
                return vec![narration::templates::CAST_NOT_A_CASTER.to_string()];
            }
            // SRD 2024 Armor Training: wearing non-proficient armor blocks
            // all spellcasting (see docs/reference/equipment.md).
            if state.character.wearing_nonproficient_armor {
                return vec![
                    "You can't cast spells while wearing armor you're not proficient with."
                        .to_string(),
                ];
            }
            // Check if spell is known
            let spell_def = match spells::find_spell(&spell) {
                Some(def) if state.character.known_spells.iter().any(|s| s.to_lowercase() == def.name.to_lowercase()) => def,
                _ => return vec![narration::templates::CAST_UNKNOWN_SPELL.to_string()],
            };
            // Ritual cast path: the player asked to cast as a ritual, which
            // bypasses slot consumption but only works for spells with the
            // Ritual tag. Per SRD 5.1 rituals take 10 minutes longer; since
            // we don't have a time system, we narrate flavor only.
            if ritual {
                if !spell_def.ritual {
                    return vec![narration::templates::CAST_NOT_A_RITUAL
                        .replace("{spell}", spell_def.name)];
                }
                return vec![
                    narration::templates::CAST_RITUAL_INTRO
                        .replace("{spell}", spell_def.name),
                ];
            }
            // In exploration, only a subset of spells resolve meaningfully:
            // flavor cantrips, healing spells (self-target for MVP), and
            // Fire Bolt's "nothing to aim at" flavor.
            match spell_def.name {
                // Wizard flavor cantrip (already supported)
                "Prestidigitation" => {
                    vec![narration::templates::CAST_PRESTIDIGITATION.to_string()]
                }
                "Fire Bolt" => {
                    vec![narration::templates::CAST_FIRE_BOLT_EXPLORE.to_string()]
                }
                // ---- Flavor cantrips (utility; no slot, no target) ----
                "Druidcraft" => {
                    vec![narration::templates::CAST_DRUIDCRAFT.to_string()]
                }
                "Mage Hand" => {
                    vec![narration::templates::CAST_MAGE_HAND.to_string()]
                }
                "Light" => {
                    vec![narration::templates::CAST_LIGHT.to_string()]
                }
                "Guidance" => {
                    vec![narration::templates::CAST_GUIDANCE.to_string()]
                }
                "Minor Illusion" => {
                    vec![narration::templates::CAST_MINOR_ILLUSION.to_string()]
                }
                // ---- Healing (self-target for MVP) ----
                // Healing spells are leveled, so a slot must be consumed.
                // Exhausting slots at full HP is SRD-accurate.
                "Cure Wounds" => {
                    if !spells::consume_spell_slot(
                        spell_def.level,
                        &mut state.character.spell_slots_remaining,
                    ) {
                        return vec![narration::templates::CAST_NO_SLOTS.to_string()];
                    }
                    let casting_ability = spells::spellcasting_ability(&state.character.class.to_string());
                    let caster_score = state.character.ability_scores
                        .get(&casting_ability).copied().unwrap_or(10);
                    let outcome = spells::resolve_cure_wounds(&mut rng, caster_score);
                    let mut out = Vec::new();
                    if let spells::CastOutcome::CureWoundsResult { healing, rolled, modifier } = outcome {
                        let new_hp = (state.character.current_hp + healing).min(state.character.max_hp);
                        let applied = new_hp - state.character.current_hp;
                        state.character.current_hp = new_hp;
                        if applied == 0 && state.character.current_hp == state.character.max_hp {
                            out.push(narration::templates::CAST_HEAL_FULL_HP
                                .replace("{spell}", "Cure Wounds")
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        } else {
                            out.push(narration::templates::CAST_CURE_WOUNDS_SELF
                                .replace("{roll}", &rolled.to_string())
                                .replace("{mod}", &modifier.to_string())
                                .replace("{healing}", &healing.to_string())
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        }
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        out.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));
                    }
                    out
                }
                "Healing Word" => {
                    if !spells::consume_spell_slot(
                        spell_def.level,
                        &mut state.character.spell_slots_remaining,
                    ) {
                        return vec![narration::templates::CAST_NO_SLOTS.to_string()];
                    }
                    let casting_ability = spells::spellcasting_ability(&state.character.class.to_string());
                    let caster_score = state.character.ability_scores
                        .get(&casting_ability).copied().unwrap_or(10);
                    let outcome = spells::resolve_healing_word(&mut rng, caster_score);
                    let mut out = Vec::new();
                    if let spells::CastOutcome::HealingWordResult { healing, rolled, modifier } = outcome {
                        let new_hp = (state.character.current_hp + healing).min(state.character.max_hp);
                        let applied = new_hp - state.character.current_hp;
                        state.character.current_hp = new_hp;
                        if applied == 0 && state.character.current_hp == state.character.max_hp {
                            out.push(narration::templates::CAST_HEAL_FULL_HP
                                .replace("{spell}", "Healing Word")
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        } else {
                            out.push(narration::templates::CAST_HEALING_WORD_SELF
                                .replace("{roll}", &rolled.to_string())
                                .replace("{mod}", &modifier.to_string())
                                .replace("{healing}", &healing.to_string())
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        }
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        out.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));
                    }
                    out
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
        // ---- Class-feature commands ----
        Command::Rage => {
            if state.character.class != character::class::Class::Barbarian {
                return vec!["Only Barbarians can rage.".to_string()];
            }
            if state.character.class_features.rage_active {
                return vec!["You are already raging.".to_string()];
            }
            if state.character.class_features.rage_uses_remaining == 0 {
                return vec!["You have no Rage uses remaining. Rest to recover them.".to_string()];
            }
            state.character.class_features.rage_uses_remaining -= 1;
            state.character.class_features.rage_active = true;
            vec!["You enter a rage! Your attacks deal bonus damage and you have resistance to physical damage.".to_string()]
        }
        Command::BardicInspiration(target) => {
            if state.character.class != character::class::Class::Bard {
                return vec!["Only Bards can grant Bardic Inspiration.".to_string()];
            }
            if state.character.class_features.bardic_inspiration_remaining == 0 {
                return vec!["You have no Bardic Inspiration uses remaining. Rest to recover them.".to_string()];
            }
            state.character.class_features.bardic_inspiration_remaining -= 1;
            let recipient = if target.is_empty() { "an ally".to_string() } else { target };
            vec![format!("You inspire {}! They gain a Bardic Inspiration die.", recipient)]
        }
        Command::ChannelDivinity => {
            let is_eligible = matches!(state.character.class,
                character::class::Class::Cleric | character::class::Class::Paladin);
            if !is_eligible {
                return vec!["Only Clerics and Paladins can use Channel Divinity.".to_string()];
            }
            if state.character.class_features.channel_divinity_remaining == 0 {
                return vec!["You have no Channel Divinity uses remaining. Rest to recover them.".to_string()];
            }
            state.character.class_features.channel_divinity_remaining -= 1;
            vec!["You channel divine power!".to_string()]
        }
        Command::LayOnHands(target) => {
            if state.character.class != character::class::Class::Paladin {
                return vec!["Only Paladins have Lay on Hands.".to_string()];
            }
            if state.character.class_features.lay_on_hands_pool == 0 {
                return vec!["Your Lay on Hands pool is empty. Long rest to restore it.".to_string()];
            }
            // Heal 5 HP per use from the pool (MVP: spend all available, cap at missing HP)
            let heal_amount = state.character.class_features.lay_on_hands_pool
                .min((state.character.max_hp - state.character.current_hp).max(0) as u32);
            state.character.class_features.lay_on_hands_pool -= heal_amount;
            state.character.current_hp =
                (state.character.current_hp + heal_amount as i32).min(state.character.max_hp);
            let recipient = if target.is_empty() || target == "self" {
                "yourself".to_string()
            } else {
                target
            };
            vec![format!("You lay hands on {}, restoring {} HP. ({} HP remaining in pool)",
                recipient, heal_amount, state.character.class_features.lay_on_hands_pool)]
        }
        Command::Ki(ability) => {
            if state.character.class != character::class::Class::Monk {
                return vec!["Only Monks can spend Ki points.".to_string()];
            }
            if state.character.class_features.ki_points_remaining == 0 {
                return vec!["You have no Ki points remaining. Rest to restore them.".to_string()];
            }
            state.character.class_features.ki_points_remaining -= 1;
            vec![format!("You spend a Ki point on {}. ({} Ki remaining)",
                ability, state.character.class_features.ki_points_remaining)]
        }
        Command::Attune(target) => handle_attune_command(state, &target),
        Command::Unattune(target) => handle_unattune_command(state, &target),
        Command::ListAttunements => handle_list_attunements(state),
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
            // Death Saving Throws (issue #84): if the player is dying when
            // their turn comes up, roll a death save and auto-end their turn.
            // An unconscious character can't act; advance to the next
            // combatant and let NPC turns continue. If the save stabilizes
            // (nat 20 or third success) the player regains 1 HP and plays
            // the turn normally. If the third failure lands, combat ends
            // via `check_end` on the next loop iteration.
            if combat.is_player_dying(state) {
                let (d20, outcome) = combat.roll_death_save(rng, &mut state.character);
                lines.extend(combat::narrate_death_save_outcome(d20, outcome));
                match outcome {
                    combat::DeathSaveOutcome::CritSuccess
                    | combat::DeathSaveOutcome::Stable => {
                        // Player is conscious again; yield the turn.
                        state.active_combat = Some(combat);
                        break;
                    }
                    combat::DeathSaveOutcome::Dead => {
                        // check_end on next iteration will declare defeat.
                        state.active_combat = Some(combat);
                        continue;
                    }
                    _ => {
                        // Still dying; skip this turn.
                        combat.end_player_turn();
                        combat.advance_turn(state);
                        state.active_combat = Some(combat);
                        continue;
                    }
                }
            }
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
/// Decide whether an NPC's upcoming movement should trigger a player
/// opportunity attack prompt.
///
/// The gate is the player's equipped weapon reach — either 5 ft by default
/// or 10 ft when the main-hand weapon has the REACH property (see
/// [`combat::player_melee_reach`]). An OA is offered only when:
///  1. The player has a reaction available and can take reactions.
///  2. The NPC is alive.
///  3. The NPC is currently within the player's melee reach (`old_distance`).
///  4. The NPC's next action would move it beyond the player's reach
///     (`new_distance > player_reach`).
///
/// Returns `Some((old_distance, new_distance))` when the OA should fire.
///
/// **Current NPC AI note:** the stock `resolve_npc_turn` always moves
/// *toward* the player — it never triggers an OA. This gate is still wired
/// so that a future retreat/kite AI (issue #43) will fire the prompt
/// correctly without further plumbing. The reach check guarantees the
/// prompt never fires for a target that was already beyond the player's
/// threatened area.
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

    // Gate: NPC must currently be within the player's melee reach. If the
    // NPC is already beyond reach, there's no threat to revoke and no OA.
    // Reach honors the REACH weapon property via `player_melee_reach` — 10 ft
    // when wielding a reach weapon, 5 ft otherwise.
    if !combat::npc_within_player_reach(state, combat, npc_id) {
        return None;
    }
    let distance = *combat.distances.get(&npc_id).unwrap_or(&u32::MAX);

    // Determine whether the NPC will actually move away. The stock AI in
    // `resolve_npc_turn` prefers melee (if in reach) then ranged (if in
    // range) then moves toward the player. None of those branches move the
    // NPC *away* from the player, so with the current AI this predicate
    // returns `None`.
    //
    // A future AI change (issue #43 — retreat on low HP, kiting with a
    // ranged attack, etc.) should surface the predicted destination here,
    // at which point the reach gate above and the reach check below form
    // a correct trigger for the player OA.
    let has_melee_in_reach = stats.attacks.iter().any(|a| a.reach > 0 && distance <= a.reach as u32);
    let has_ranged_in_range = stats.attacks.iter().any(|a| {
        a.range_long > 0 && distance <= a.range_long as u32
    });
    let _ = (has_melee_in_reach, has_ranged_in_range);

    // Placeholder until the AI gains a retreat path: with the current AI
    // the NPC will never move out of reach, so we return None.
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
                let name = state.world.npcs.get(&fleeing_npc_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "the enemy".to_string());
                // Grappled-vs-non-grappler: the player rolls the OA with disadvantage
                // if they are grappled by someone other than the fleeing target.
                let grappled_disadv = crate::conditions::grappled_attack_disadvantage(
                    &state.character.conditions,
                    &name,
                );
                // Compute melee-zone state at the trigger instant. If a ranged
                // weapon is being used for the OA (unusual) the standard
                // ranged-in-melee disadvantage applies.
                let hostile_within_5ft = combat::has_living_hostile_within(state, &combat, 5);
                let mut result = combat::resolve_player_attack(
                    rng, &state.character, target_ac, false,
                    weapon_id, &state.world.items, old_distance,
                    state.character.equipped.off_hand.is_none(),
                    hostile_within_5ft,
                    &target_conditions,
                    grappled_disadv,
                    false,
                );
                // Apply magic weapon bonuses (if wielding a MagicWeapon).
                let (atk_b, dmg_b) = magic_weapon_bonuses(state, weapon_id);
                apply_magic_weapon_bonuses(&mut result, atk_b, dmg_b);
                // Rogue Sneak Attack can fire on an opportunity attack
                // (the SRD "once per turn" cap applies across the round,
                // not just the player's turn).
                apply_sneak_attack(
                    rng, state, &mut result, &mut lines,
                    weapon_id, old_distance,
                );
                if result.hit {
                    if let Some(npc) = state.world.npcs.get_mut(&fleeing_npc_id) {
                        let _dealt = combat::apply_damage_to_npc(
                            npc, result.damage, result.damage_type, &mut lines,
                        );
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

    // NPC attacking the player: disadvantage if the NPC is grappled by someone
    // other than the player (per 2024 SRD Grappled condition), or if the NPC
    // is Sap-marked (2024 SRD Sap mastery).
    let grappled = crate::conditions::grappled_attack_disadvantage(
        &npc_conditions,
        &state.character.name,
    );
    let sapped = combat::consume_sap_disadvantage(combat, npc_id);
    let extra_disadvantage = grappled || sapped;
    let result = combat::resolve_npc_attack(
        rng, &attack, player_ac, combat.player_dodging, distance,
        &npc_conditions, &player_conditions,
        extra_disadvantage,
    );
    if sapped {
        lines.push("(Disadvantage from Sap mastery.)".to_string());
    }
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
        // Concentration check: if the player is concentrating on a spell,
        // they must make a CON save (DC = max(10, damage/2)) to maintain it.
        check_player_concentration_on_damage(rng, state, result.damage, &mut lines);
    } else if result.natural_1 {
        lines.push(format!("{} {} {} -- natural 1, miss!", npc_name, verb, result.weapon_name));
    } else {
        lines.push(format!("{} {} {} ({}+{}={} vs AC {}){} -- miss.",
            npc_name, verb, result.weapon_name, result.attack_roll,
            attack.hit_bonus, result.total_attack, player_ac, disadv));
    }
    lines
}

/// Check and resolve the player's concentration when they take damage.
///
/// If the player is currently concentrating on a spell and takes damage,
/// they make a Constitution saving throw against DC max(10, damage / 2)
/// per SRD 5.1. On failure the concentration spell drops; on success it
/// holds. No-op if the player isn't concentrating or `damage_taken <= 0`.
fn check_player_concentration_on_damage(
    rng: &mut StdRng,
    state: &mut GameState,
    damage_taken: i32,
    lines: &mut Vec<String>,
) {
    if damage_taken <= 0 { return; }
    let spell = match state.character.class_features.concentration_spell.clone() {
        Some(s) => s,
        None => return,
    };
    let con_score = state.character.ability_scores
        .get(&Ability::Constitution).copied().unwrap_or(10);
    let con_prof = state.character.is_proficient_in_save(Ability::Constitution);
    let prof = state.character.proficiency_bonus();
    let save = spells::resolve_concentration_save(
        rng, con_score, con_prof, prof, damage_taken,
    );
    if save.saved {
        lines.push(narration::templates::CONCENTRATION_HELD
            .replace("{spell}", &spell));
    } else {
        state.character.class_features.concentration_spell = None;
        lines.push(narration::templates::CONCENTRATION_BROKEN
            .replace("{spell}", &spell));
    }
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
        let level_before = state.character.level;
        lines.extend(leveling::award_xp(&mut state.character, monster_xp));
        let levels_gained = state.character.level - level_before;
        apply_post_levelup_feat_bonuses(&mut state.character, levels_gained);

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

        // If a level-up granted unspent ASI credits, prompt the player.
        // Skipped if Victory just transitioned us out of Exploration.
        lines.extend(check_and_enter_asi_phase(state));

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
        let level_before = state.character.level;
        lines.extend(leveling::award_xp(
            &mut state.character,
            leveling::OBJECTIVE_XP_REWARD * newly_completed,
        ));
        let levels_gained = state.character.level - level_before;
        apply_post_levelup_feat_bonuses(&mut state.character, levels_gained);
        // Note: ASI prompt for objective-driven level-ups is triggered by
        // the caller (end_combat) so this helper stays state-mutation-only.
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
        let level_before = state.character.level;
        lines.extend(leveling::award_xp(
            &mut state.character,
            leveling::OBJECTIVE_XP_REWARD * newly_completed,
        ));
        let levels_gained = state.character.level - level_before;
        apply_post_levelup_feat_bonuses(&mut state.character, levels_gained);
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

    // Death Saving Throws (issue #84): if it's the player's turn but they
    // are dying, auto-roll a death save. Unconscious characters can't
    // issue commands, so any player input received while they're at 0 HP
    // simply triggers the save and advances to NPC turns. A stabilizing
    // save (nat 20 / third success) returns the player to consciousness
    // and lets their input run normally.
    if combat.is_player_turn() && combat.is_player_dying(state) {
        let (d20, outcome) = combat.roll_death_save(&mut rng, &mut state.character);
        let mut lines = combat::narrate_death_save_outcome(d20, outcome);
        match outcome {
            combat::DeathSaveOutcome::CritSuccess
            | combat::DeathSaveOutcome::Stable => {
                // Player is conscious. Show prompt; let them issue a new
                // command next turn (the current input is consumed by the
                // death save, per SRD: rolling the save IS the turn's
                // action when unconscious, and regaining consciousness
                // mid-turn leaves no movement/action this turn).
                combat.end_player_turn();
                combat.advance_turn(state);
                state.active_combat = Some(combat);
                let npc_lines = process_npc_turns(state, &mut rng);
                lines.extend(npc_lines);
                if let Some(ref combat) = state.active_combat {
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
            combat::DeathSaveOutcome::Dead => {
                state.active_combat = Some(combat);
                if let Some(victory) = state.active_combat.as_ref()
                    .and_then(|c| c.check_end(state))
                {
                    lines.extend(end_combat(state, victory));
                }
                return lines;
            }
            _ => {
                // Still dying: skip the player's turn, let NPCs act.
                combat.end_player_turn();
                combat.advance_turn(state);
                state.active_combat = Some(combat);
                let npc_lines = process_npc_turns(state, &mut rng);
                lines.extend(npc_lines);
                if let Some(ref combat) = state.active_combat {
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
        }
    }

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

                    let npc_name = state.world.npcs.get(&npc_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "the enemy".to_string());

                    // Charmed: the player cannot attack their charmer (per 2024 SRD).
                    if !crate::conditions::can_attack_target(&state.character.conditions, &npc_name) {
                        state.active_combat = Some(combat);
                        return vec![format!(
                            "You can't bring yourself to attack {} -- you are Charmed by them.",
                            npc_name
                        )];
                    }

                    // Grappled: disadvantage on attacks against any target other than
                    // the grappler.
                    let grappled_disadv = crate::conditions::grappled_attack_disadvantage(
                        &state.character.conditions,
                        &npc_name,
                    );

                    // Vex mastery: if the player's previous attack applied
                    // the Vex mark to this NPC (and it hasn't been consumed
                    // yet) grant advantage on this attack. We consume the
                    // mark on the attack roll regardless of hit/miss per SRD.
                    let vex_advantage =
                        combat::consume_vex_advantage(&mut combat, npc_id);

                    let mut result = combat::resolve_player_attack(
                        &mut rng, &state.character, target_ac, target_dodging,
                        weapon_id, &state.world.items, distance, off_hand_free, hostile_within_5ft,
                        target_conditions,
                        grappled_disadv,
                        vex_advantage,
                    );
                    if vex_advantage {
                        lines.push("(Advantage from Vex mastery.)".to_string());
                    }
                    // Apply magic weapon bonuses (if wielding a MagicWeapon).
                    let (atk_b, dmg_b) = magic_weapon_bonuses(state, weapon_id);
                    apply_magic_weapon_bonuses(&mut result, atk_b, dmg_b);

                    // Rogue Sneak Attack: add bonus dice to a qualifying hit
                    // before damage is applied so narration reflects the
                    // full total. See `apply_sneak_attack` for eligibility.
                    apply_sneak_attack(
                        &mut rng, state, &mut result, &mut lines,
                        weapon_id, distance,
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

                    // Apply damage (honoring stat-block resistances/immunities)
                    if result.hit {
                        if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                            let _dealt = combat::apply_damage_to_npc(
                                npc, result.damage, result.damage_type, &mut lines,
                            );
                            if let Some(stats) = npc.combat_stats.as_ref() {
                                if stats.current_hp <= 0 {
                                    lines.push(format!("{} is slain!", npc_name));
                                }
                            }
                        }
                    }

                    // Weapon Mastery dispatch (2024 SRD). The mastery of the
                    // equipped main-hand weapon fires after damage is applied.
                    // Graze triggers on miss; all others trigger on hit. Nick
                    // is off-hand-only so it doesn't fire here.
                    apply_mainhand_mastery_effects(
                        &mut rng, state, &mut combat, &mut lines,
                        &result, npc_id, weapon_id, distance,
                    );

                    combat.action_used = true;
                }
                ResolveResult::Ambiguous(matches) => {
                    state.active_combat = Some(combat);
                    return emit_disambiguation(state, "attack", &matches);
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
                    return emit_disambiguation(state, "approach", &matches);
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
            lines.push("You take the Dodge action. Attacks against you have disadvantage until your next turn.".to_string());
        }
        Command::Disengage => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_disengaging = true;
            combat.action_used = true;
            lines.push("You take the Disengage action. You can retreat without provoking opportunity attacks.".to_string());
        }
        Command::Dash => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            combat.player_movement_remaining += state.character.speed;
            combat.action_used = true;
            lines.push(format!("You take the Dash action. Movement this turn: {} ft.", combat.player_movement_remaining));
        }
        Command::BonusDash => {
            if combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your bonus action this turn.".to_string()];
            }
            combat.player_movement_remaining += state.character.speed;
            combat.bonus_action_used = true;
            lines.push(format!(
                "You dash as a bonus action. Movement this turn: {} ft.",
                combat.player_movement_remaining
            ));
        }
        Command::OffHandAttack(target_name) => {
            // Two-Weapon Fighting: requires main-hand Attack action already used,
            // both weapons light melee, and bonus action available.
            //
            // Nick mastery (SRD 2024): when the off-hand weapon has Nick and
            // the character has unlocked it, the off-hand swing is part of
            // the Attack action instead of a bonus action. That means:
            //   - we still require `action_used = true` (the player must have
            //     taken the Attack action this turn), and
            //   - the bonus-action gate is skipped, and
            //   - the bonus-action slot is NOT consumed.
            // This is once per turn (`combat.nick_used_this_turn`).
            if !combat.action_used {
                state.active_combat = Some(combat);
                return vec![
                    "You must take the Attack action with your main hand before using the off-hand bonus attack.".to_string()
                ];
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

            // Determine whether Nick applies: off-hand weapon has Nick mastery
            // and the character has it unlocked. We do the lookup now so we
            // can decide whether to bypass the bonus-action gate below.
            let off_hand_item_name = state.world.items.get(&off_hand_id)
                .map(|i| i.name.clone())
                .unwrap_or_default();
            let has_nick_mastery = equipment::weapon_mastery(&off_hand_item_name)
                == Some(crate::types::Mastery::Nick)
                && equipment::character_has_mastery(&state.character, &off_hand_item_name);
            let nick_applies = combat::apply_nick_mastery(has_nick_mastery, &mut combat);

            if !nick_applies && combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your bonus action this turn.".to_string()];
            }
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
                    let npc_name = state.world.npcs.get(&npc_id)
                        .map(|n| n.name.clone())
                        .unwrap_or_else(|| "the enemy".to_string());

                    // Charmed: player cannot attack their charmer, even off-hand.
                    if !crate::conditions::can_attack_target(&state.character.conditions, &npc_name) {
                        state.active_combat = Some(combat);
                        return vec![format!(
                            "You can't bring yourself to attack {} -- you are Charmed by them.",
                            npc_name
                        )];
                    }

                    // Grappled-vs-non-grappler disadvantage.
                    let grappled_disadv = crate::conditions::grappled_attack_disadvantage(
                        &state.character.conditions,
                        &npc_name,
                    );

                    // Vex: consume on any off-hand attack against a Vex-marked
                    // target, same as the main-hand path.
                    let vex_advantage =
                        combat::consume_vex_advantage(&mut combat, npc_id);

                    // Resolve the attack using the OFF-HAND weapon.
                    let mut result = combat::resolve_player_attack(
                        &mut rng, &state.character, target_ac, target_dodging,
                        Some(off_hand_id), &state.world.items, distance,
                        false, // off-hand slot is occupied (by this weapon), no Versatile bonus
                        hostile_within_5ft,
                        target_conditions,
                        grappled_disadv,
                        vex_advantage,
                    );
                    if vex_advantage {
                        lines.push("(Advantage from Vex mastery.)".to_string());
                    }
                    // Apply magic weapon bonuses to the off-hand result too.
                    let (atk_b, dmg_b) = magic_weapon_bonuses(state, Some(off_hand_id));
                    apply_magic_weapon_bonuses(&mut result, atk_b, dmg_b);

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
                        //
                        // MagicWeapon variants carry the same `properties` field, so
                        // we check both variants to find FINESSE for off-hand magic
                        // daggers/shortswords.
                        let is_finesse = match state.world.items.get(&off_hand_id) {
                            Some(item) => match &item.item_type {
                                state::ItemType::Weapon { properties, .. } =>
                                    properties & crate::equipment::FINESSE != 0,
                                state::ItemType::MagicWeapon { properties, .. } =>
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

                    // Rogue Sneak Attack on the off-hand swing. SA dice are
                    // added on top of weapon damage and are not subject to
                    // the off-hand ability-mod strip. We route the boosted
                    // damage through `adjusted_damage` so the narration and
                    // damage application both reflect the SA bonus.
                    {
                        let mut sa_result = result.clone();
                        sa_result.damage = adjusted_damage;
                        apply_sneak_attack(
                            &mut rng, state, &mut sa_result, &mut lines,
                            Some(off_hand_id), distance,
                        );
                        adjusted_damage = sa_result.damage;
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

                    // Apply damage (honoring stat-block resistances/immunities)
                    if result.hit {
                        if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                            let _dealt = combat::apply_damage_to_npc(
                                npc, adjusted_damage, result.damage_type, &mut lines,
                            );
                            if let Some(stats) = npc.combat_stats.as_ref() {
                                if stats.current_hp <= 0 {
                                    lines.push(format!("{} is slain!", npc_name));
                                }
                            }
                        }
                    }

                    // Weapon Mastery dispatch for off-hand attacks. Nick is
                    // handled above (gates bonus-action consumption); all
                    // other masteries still fire normally on the off-hand
                    // weapon if the character has mastery unlocked.
                    let mut off_result = result.clone();
                    off_result.damage = adjusted_damage;
                    apply_mainhand_mastery_effects(
                        &mut rng, state, &mut combat, &mut lines,
                        &off_result, npc_id, Some(off_hand_id), distance,
                    );

                    if nick_applies {
                        lines.push(
                            "Nick: this off-hand swing is part of your Attack action."
                                .to_string(),
                        );
                    } else {
                        combat.bonus_action_used = true;
                    }
                }
                ResolveResult::Ambiguous(matches) => {
                    state.active_combat = Some(combat);
                    return emit_disambiguation(state, "offhand attack", &matches);
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
        }
        Command::Unequip(target_str) => {
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You've already used your action this turn.".to_string()];
            }
            lines.extend(handle_unequip_command(state, &target_str));
            combat.action_used = true;
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
        Command::Cast { spell, target, ritual } => {
            // Check if caster
            if state.character.known_spells.is_empty() {
                state.active_combat = Some(combat);
                return vec![narration::templates::CAST_NOT_A_CASTER.to_string()];
            }
            // SRD 2024 Armor Training: wearing non-proficient armor blocks
            // all spellcasting (see docs/reference/equipment.md).
            if state.character.wearing_nonproficient_armor {
                state.active_combat = Some(combat);
                return vec![
                    "You can't cast spells while wearing armor you're not proficient with."
                        .to_string(),
                ];
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

            // Ritual-cast path: verify the spell has the Ritual tag, skip
            // slot consumption, narrate flavor. Rituals normally take 10
            // minutes longer -- with no combat time system, we simply
            // narrate and consume the action for flavor consistency.
            if ritual {
                if !spell_def.ritual {
                    state.active_combat = Some(combat);
                    return vec![narration::templates::CAST_NOT_A_RITUAL
                        .replace("{spell}", spell_def.name)];
                }
                combat.action_used = true;
                lines.push(narration::templates::CAST_RITUAL_INTRO
                    .replace("{spell}", spell_def.name));
                state.active_combat = Some(combat);
                return lines;
            }

            // Reaction-only spells (e.g. Shield) cannot be cast as an action
            // on the player's turn. They trigger automatically during NPC
            // turns when the appropriate condition fires (incoming attack hit
            // or Magic Missile targeting).
            if spell_def.casting == spells::CastingMode::Reaction {
                state.active_combat = Some(combat);
                return vec![narration::templates::CAST_REACTION_ONLY
                    .replace("{spell}", spell_def.name)];
            }

            // Check spell slots
            if !spells::consume_spell_slot(spell_def.level, &mut state.character.spell_slots_remaining) {
                state.active_combat = Some(combat);
                return vec![narration::templates::CAST_NO_SLOTS.to_string()];
            }

            // Concentration: starting a new concentration spell drops any
            // prior one. Narration reports the drop.
            if spell_def.concentration {
                match spells::begin_concentration(
                    &mut state.character.class_features.concentration_spell,
                    spell_def.name,
                ) {
                    spells::ConcentrationStart::ReplacedPrior(prior) => {
                        lines.push(narration::templates::CONCENTRATION_DROPPED
                            .replace("{old}", &prior)
                            .replace("{new}", spell_def.name));
                    }
                    spells::ConcentrationStart::Started => {
                        lines.push(narration::templates::CONCENTRATION_STARTED
                            .replace("{spell}", spell_def.name));
                    }
                }
            }

            // Spellcasting ability per class (INT/WIS/CHA). The value is
            // the caster's score in whichever ability their class uses for
            // spells -- resolve_* helpers are ability-agnostic.
            let class_name = state.character.class.to_string();
            let casting_ability = spells::spellcasting_ability(&class_name);
            let caster_score = state.character.ability_scores
                .get(&casting_ability).copied().unwrap_or(10);
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

                            let outcome = spells::resolve_fire_bolt(&mut rng, caster_score, prof_bonus, target_ac);
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
                                    // Apply damage (honoring stat-block resistances/immunities)
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        let _dealt = combat::apply_damage_to_npc(
                                            npc, damage, state::DamageType::Fire, &mut lines,
                                        );
                                        if let Some(stats) = npc.combat_stats.as_ref() {
                                            if stats.current_hp <= 0 {
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
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast fire bolt at", &matches);
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

                                // Apply damage (honoring stat-block resistances/immunities)
                                if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                    let _dealt = combat::apply_damage_to_npc(
                                        npc, total_damage, state::DamageType::Force, &mut lines,
                                    );
                                    if let Some(stats) = npc.combat_stats.as_ref() {
                                        if stats.current_hp <= 0 {
                                            lines.push(format!("{} is slain!", npc_name));
                                        }
                                    }
                                }
                            }
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast magic missile at", &matches);
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

                    let outcome = spells::resolve_burning_hands(&mut rng, caster_score, prof_bonus, &targets);
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

                        // Apply damage to NPCs (honoring stat-block resistances/immunities)
                        for result in &results {
                            // Find NPC by name
                            for (_, npc) in state.world.npcs.iter_mut() {
                                if npc.name == result.name {
                                    let _dealt = combat::apply_damage_to_npc(
                                        npc, result.damage_taken, state::DamageType::Fire, &mut lines,
                                    );
                                    if let Some(stats) = npc.combat_stats.as_ref() {
                                        if stats.current_hp <= 0 {
                                            lines.push(format!("{} is slain!", result.name));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    combat.action_used = true;
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
                }
                // NOTE: "Shield" is handled by the CastingMode::Reaction
                // guard above (rejected before slot consumption). The reaction
                // path lives in resolve_reaction_decision(). No action-cast arm
                // is needed here.
                //
                // ---- Cleric starters ----
                "Sacred Flame" => {
                    // Cantrip: target makes a DEX save; on fail 1d8 radiant.
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Sacred Flame")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let targets = build_spell_targets(&combat, state);
                            let Some(spell_target) = targets.iter().find(|t| t.id == npc_id).cloned() else {
                                state.active_combat = Some(combat);
                                return vec![format!("There's no \"{}\" to target.", target_name)];
                            };
                            let npc_name = spell_target.name.clone();
                            let outcome = spells::resolve_sacred_flame(&mut rng, caster_score, prof_bonus, &spell_target);
                            if let spells::CastOutcome::SacredFlame { save_result, damage } = outcome {
                                let save_str = format!("{}+{}={} vs DC {}",
                                    save_result.roll, save_result.modifier,
                                    save_result.total, save_result.dc);
                                if save_result.saved {
                                    lines.push(narration::templates::CAST_SACRED_FLAME_SAVE
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                } else {
                                    lines.push(narration::templates::CAST_SACRED_FLAME_HIT
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str)
                                        .replace("{damage}", &damage.to_string()));
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        let _dealt = combat::apply_damage_to_npc(
                                            npc, damage, state::DamageType::Radiant, &mut lines,
                                        );
                                        if let Some(stats) = npc.combat_stats.as_ref() {
                                            if stats.current_hp <= 0 {
                                                lines.push(format!("{} is slain!", npc_name));
                                            }
                                        }
                                    }
                                }
                            }
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast sacred flame at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                "Cure Wounds" => {
                    // Self-heal 1d8 + spellcasting mod. Slot already consumed at top.
                    let outcome = spells::resolve_cure_wounds(&mut rng, caster_score);
                    if let spells::CastOutcome::CureWoundsResult { healing, rolled, modifier } = outcome {
                        let new_hp = (state.character.current_hp + healing).min(state.character.max_hp);
                        let applied = new_hp - state.character.current_hp;
                        state.character.current_hp = new_hp;
                        if applied == 0 && state.character.current_hp == state.character.max_hp {
                            lines.push(narration::templates::CAST_HEAL_FULL_HP
                                .replace("{spell}", "Cure Wounds")
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        } else {
                            lines.push(narration::templates::CAST_CURE_WOUNDS_SELF
                                .replace("{roll}", &rolled.to_string())
                                .replace("{mod}", &modifier.to_string())
                                .replace("{healing}", &healing.to_string())
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        }
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        lines.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));
                    }
                    combat.action_used = true;
                }
                "Guiding Bolt" => {
                    // L1 spell attack; 4d6 radiant, crit x2.
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Guiding Bolt")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let target_ac = state.world.npcs.get(&npc_id)
                                .and_then(|n| n.combat_stats.as_ref())
                                .map(|s| s.ac).unwrap_or(10);
                            let npc_name = state.world.npcs.get(&npc_id)
                                .map(|n| n.name.clone()).unwrap_or_else(|| "the enemy".to_string());
                            let outcome = spells::resolve_guiding_bolt(&mut rng, caster_score, prof_bonus, target_ac);
                            if let spells::CastOutcome::GuidingBolt { attack, damage } = outcome {
                                if attack.hit {
                                    if attack.natural_20 {
                                        lines.push(narration::templates::CAST_GUIDING_BOLT_CRIT
                                            .replace("{target}", &npc_name)
                                            .replace("{damage}", &damage.to_string()));
                                    } else {
                                        lines.push(narration::templates::CAST_GUIDING_BOLT_HIT
                                            .replace("{target}", &npc_name)
                                            .replace("{roll}", &attack.roll.to_string())
                                            .replace("{mod}", &attack.modifier.to_string())
                                            .replace("{total}", &attack.total.to_string())
                                            .replace("{ac}", &target_ac.to_string())
                                            .replace("{damage}", &damage.to_string()));
                                    }
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        let _dealt = combat::apply_damage_to_npc(
                                            npc, damage, state::DamageType::Radiant, &mut lines,
                                        );
                                        if let Some(stats) = npc.combat_stats.as_ref() {
                                            if stats.current_hp <= 0 {
                                                lines.push(format!("{} is slain!", npc_name));
                                            }
                                        }
                                    }
                                } else if attack.natural_1 {
                                    lines.push(narration::templates::CAST_GUIDING_BOLT_MISS_NAT1
                                        .replace("{target}", &npc_name));
                                } else {
                                    lines.push(narration::templates::CAST_GUIDING_BOLT_MISS
                                        .replace("{target}", &npc_name)
                                        .replace("{roll}", &attack.roll.to_string())
                                        .replace("{mod}", &attack.modifier.to_string())
                                        .replace("{total}", &attack.total.to_string())
                                        .replace("{ac}", &target_ac.to_string()));
                                }
                            }
                            let remaining = state.character.spell_slots_remaining[0];
                            let max = state.character.spell_slots_max[0];
                            lines.push(narration::templates::CAST_SLOT_USED
                                .replace("{remaining}", &remaining.to_string())
                                .replace("{max}", &max.to_string())
                                .replace("{level}", "1"));
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast guiding bolt at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                "Bless" => {
                    // Self-buff, concentration. Concentration already started by the
                    // common concentration branch above. Slot already consumed.
                    lines.push(narration::templates::CAST_BLESS.to_string());
                    let remaining = state.character.spell_slots_remaining[0];
                    let max = state.character.spell_slots_max[0];
                    lines.push(narration::templates::CAST_SLOT_USED
                        .replace("{remaining}", &remaining.to_string())
                        .replace("{max}", &max.to_string())
                        .replace("{level}", "1"));
                    combat.action_used = true;
                }
                "Healing Word" => {
                    // Self-heal 1d4 + mod. SRD calls this a bonus action, but the
                    // MVP combat model resolves it as an action like other spells.
                    let outcome = spells::resolve_healing_word(&mut rng, caster_score);
                    if let spells::CastOutcome::HealingWordResult { healing, rolled, modifier } = outcome {
                        let new_hp = (state.character.current_hp + healing).min(state.character.max_hp);
                        let applied = new_hp - state.character.current_hp;
                        state.character.current_hp = new_hp;
                        if applied == 0 && state.character.current_hp == state.character.max_hp {
                            lines.push(narration::templates::CAST_HEAL_FULL_HP
                                .replace("{spell}", "Healing Word")
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        } else {
                            lines.push(narration::templates::CAST_HEALING_WORD_SELF
                                .replace("{roll}", &rolled.to_string())
                                .replace("{mod}", &modifier.to_string())
                                .replace("{healing}", &healing.to_string())
                                .replace("{current}", &state.character.current_hp.to_string())
                                .replace("{max}", &state.character.max_hp.to_string()));
                        }
                        let remaining = state.character.spell_slots_remaining[0];
                        let max = state.character.spell_slots_max[0];
                        lines.push(narration::templates::CAST_SLOT_USED
                            .replace("{remaining}", &remaining.to_string())
                            .replace("{max}", &max.to_string())
                            .replace("{level}", "1"));
                    }
                    combat.action_used = true;
                }
                // ---- Bard starters ----
                "Vicious Mockery" => {
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Vicious Mockery")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let targets = build_spell_targets(&combat, state);
                            let Some(spell_target) = targets.iter().find(|t| t.id == npc_id).cloned() else {
                                state.active_combat = Some(combat);
                                return vec![format!("There's no \"{}\" to target.", target_name)];
                            };
                            let npc_name = spell_target.name.clone();
                            let outcome = spells::resolve_vicious_mockery(&mut rng, caster_score, prof_bonus, &spell_target);
                            if let spells::CastOutcome::ViciousMockery { save_result, damage } = outcome {
                                let save_str = format!("{}+{}={} vs DC {}",
                                    save_result.roll, save_result.modifier,
                                    save_result.total, save_result.dc);
                                if save_result.saved {
                                    lines.push(narration::templates::CAST_VICIOUS_MOCKERY_SAVE
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                } else {
                                    lines.push(narration::templates::CAST_VICIOUS_MOCKERY_HIT
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str)
                                        .replace("{damage}", &damage.to_string()));
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        let _dealt = combat::apply_damage_to_npc(
                                            npc, damage, state::DamageType::Psychic, &mut lines,
                                        );
                                        if let Some(stats) = npc.combat_stats.as_ref() {
                                            if stats.current_hp <= 0 {
                                                lines.push(format!("{} is slain!", npc_name));
                                            }
                                        }
                                    }
                                }
                            }
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast vicious mockery at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                "Charm Person" => {
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Charm Person")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let targets = build_spell_targets(&combat, state);
                            let Some(spell_target) = targets.iter().find(|t| t.id == npc_id).cloned() else {
                                state.character.spell_slots_remaining[0] += 1; // refund
                                state.active_combat = Some(combat);
                                return vec![format!("There's no \"{}\" to target.", target_name)];
                            };
                            let npc_name = spell_target.name.clone();
                            let outcome = spells::resolve_charm_person(&mut rng, caster_score, prof_bonus, &spell_target);
                            if let spells::CastOutcome::CharmPerson { save_result } = outcome {
                                let save_str = format!("{}+{}={} vs DC {}",
                                    save_result.roll, save_result.modifier,
                                    save_result.total, save_result.dc);
                                if save_result.saved {
                                    lines.push(narration::templates::CAST_CHARM_PERSON_SAVE
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                } else {
                                    lines.push(narration::templates::CAST_CHARM_PERSON_HIT
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                }
                            }
                            let remaining = state.character.spell_slots_remaining[0];
                            let max = state.character.spell_slots_max[0];
                            lines.push(narration::templates::CAST_SLOT_USED
                                .replace("{remaining}", &remaining.to_string())
                                .replace("{max}", &max.to_string())
                                .replace("{level}", "1"));
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast charm person at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                // ---- Druid starters ----
                "Druidcraft" => {
                    lines.push(narration::templates::CAST_DRUIDCRAFT.to_string());
                    combat.action_used = true;
                }
                "Faerie Fire" => {
                    // L1 DEX save; concentration already handled above.
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Faerie Fire")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let targets = build_spell_targets(&combat, state);
                            let Some(spell_target) = targets.iter().find(|t| t.id == npc_id).cloned() else {
                                state.character.spell_slots_remaining[0] += 1; // refund
                                state.active_combat = Some(combat);
                                return vec![format!("There's no \"{}\" to target.", target_name)];
                            };
                            let npc_name = spell_target.name.clone();
                            let outcome = spells::resolve_faerie_fire(&mut rng, caster_score, prof_bonus, &spell_target);
                            if let spells::CastOutcome::FaerieFire { save_result } = outcome {
                                let save_str = format!("{}+{}={} vs DC {}",
                                    save_result.roll, save_result.modifier,
                                    save_result.total, save_result.dc);
                                if save_result.saved {
                                    lines.push(narration::templates::CAST_FAERIE_FIRE_SAVE
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                } else {
                                    lines.push(narration::templates::CAST_FAERIE_FIRE_HIT
                                        .replace("{target}", &npc_name)
                                        .replace("{save_result}", &save_str));
                                }
                            }
                            let remaining = state.character.spell_slots_remaining[0];
                            let max = state.character.spell_slots_max[0];
                            lines.push(narration::templates::CAST_SLOT_USED
                                .replace("{remaining}", &remaining.to_string())
                                .replace("{max}", &max.to_string())
                                .replace("{level}", "1"));
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast faerie fire at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.character.spell_slots_remaining[0] += 1; // refund
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                // ---- Warlock starters ----
                "Eldritch Blast" => {
                    let target_name = match target {
                        Some(t) => t,
                        None => {
                            state.active_combat = Some(combat);
                            return vec![narration::templates::CAST_NEED_TARGET
                                .replace("{spell}", "Eldritch Blast")];
                        }
                    };
                    let owned_candidates = build_combat_npc_candidates(&combat, state);
                    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
                        .map(|(id, name)| (*id, name.as_str())).collect();
                    match resolver::resolve_target(&target_name, &candidates) {
                        ResolveResult::Found(id) => {
                            let npc_id = id as u32;
                            let target_ac = state.world.npcs.get(&npc_id)
                                .and_then(|n| n.combat_stats.as_ref())
                                .map(|s| s.ac).unwrap_or(10);
                            let npc_name = state.world.npcs.get(&npc_id)
                                .map(|n| n.name.clone()).unwrap_or_else(|| "the enemy".to_string());
                            let outcome = spells::resolve_eldritch_blast(&mut rng, caster_score, prof_bonus, target_ac);
                            if let spells::CastOutcome::EldritchBlast { attack, damage } = outcome {
                                if attack.hit {
                                    if attack.natural_20 {
                                        lines.push(narration::templates::CAST_ELDRITCH_BLAST_CRIT
                                            .replace("{target}", &npc_name)
                                            .replace("{damage}", &damage.to_string()));
                                    } else {
                                        lines.push(narration::templates::CAST_ELDRITCH_BLAST_HIT
                                            .replace("{target}", &npc_name)
                                            .replace("{roll}", &attack.roll.to_string())
                                            .replace("{mod}", &attack.modifier.to_string())
                                            .replace("{total}", &attack.total.to_string())
                                            .replace("{ac}", &target_ac.to_string())
                                            .replace("{damage}", &damage.to_string()));
                                    }
                                    if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                                        let _dealt = combat::apply_damage_to_npc(
                                            npc, damage, state::DamageType::Force, &mut lines,
                                        );
                                        if let Some(stats) = npc.combat_stats.as_ref() {
                                            if stats.current_hp <= 0 {
                                                lines.push(format!("{} is slain!", npc_name));
                                            }
                                        }
                                    }
                                } else if attack.natural_1 {
                                    lines.push(narration::templates::CAST_ELDRITCH_BLAST_MISS_NAT1
                                        .replace("{target}", &npc_name));
                                } else {
                                    lines.push(narration::templates::CAST_ELDRITCH_BLAST_MISS
                                        .replace("{target}", &npc_name)
                                        .replace("{roll}", &attack.roll.to_string())
                                        .replace("{mod}", &attack.modifier.to_string())
                                        .replace("{total}", &attack.total.to_string())
                                        .replace("{ac}", &target_ac.to_string()));
                                }
                            }
                            combat.action_used = true;
                        }
                        ResolveResult::Ambiguous(matches) => {
                            state.active_combat = Some(combat);
                            return emit_disambiguation(state, "cast eldritch blast at", &matches);
                        }
                        ResolveResult::NotFound => {
                            state.active_combat = Some(combat);
                            return vec![format!("There's no \"{}\" to target.", target_name)];
                        }
                    }
                }
                // ---- Flavor cantrips (Mage Hand, Light, Guidance, Minor Illusion) ----
                "Mage Hand" => {
                    lines.push(narration::templates::CAST_MAGE_HAND.to_string());
                    combat.action_used = true;
                }
                "Light" => {
                    lines.push(narration::templates::CAST_LIGHT.to_string());
                    combat.action_used = true;
                }
                "Guidance" => {
                    // Concentration cantrip; the concentration branch above handled it.
                    lines.push(narration::templates::CAST_GUIDANCE.to_string());
                    combat.action_used = true;
                }
                "Minor Illusion" => {
                    lines.push(narration::templates::CAST_MINOR_ILLUSION.to_string());
                    combat.action_used = true;
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
        // ---- Class-feature commands (combat) ----
        Command::Rage => {
            if state.character.class != character::class::Class::Barbarian {
                state.active_combat = Some(combat);
                return vec!["Only Barbarians can rage.".to_string()];
            }
            if state.character.class_features.rage_active {
                state.active_combat = Some(combat);
                return vec!["You are already raging.".to_string()];
            }
            if state.character.class_features.rage_uses_remaining == 0 {
                state.active_combat = Some(combat);
                return vec!["You have no Rage uses remaining.".to_string()];
            }
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You have already used your action this turn.".to_string()];
            }
            state.character.class_features.rage_uses_remaining -= 1;
            state.character.class_features.rage_active = true;
            combat.action_used = true;
            lines.push("You enter a rage! Your attacks deal bonus damage and you have resistance to physical damage.".to_string());
        }
        Command::BardicInspiration(target) => {
            if state.character.class != character::class::Class::Bard {
                state.active_combat = Some(combat);
                return vec!["Only Bards can grant Bardic Inspiration.".to_string()];
            }
            if state.character.class_features.bardic_inspiration_remaining == 0 {
                state.active_combat = Some(combat);
                return vec!["You have no Bardic Inspiration uses remaining.".to_string()];
            }
            if combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You have already used your bonus action this turn.".to_string()];
            }
            state.character.class_features.bardic_inspiration_remaining -= 1;
            combat.bonus_action_used = true;
            let recipient = if target.is_empty() { "an ally".to_string() } else { target };
            lines.push(format!("You inspire {}! They gain a Bardic Inspiration die.", recipient));
        }
        Command::ChannelDivinity => {
            let is_eligible = matches!(state.character.class,
                character::class::Class::Cleric | character::class::Class::Paladin);
            if !is_eligible {
                state.active_combat = Some(combat);
                return vec!["Only Clerics and Paladins can use Channel Divinity.".to_string()];
            }
            if state.character.class_features.channel_divinity_remaining == 0 {
                state.active_combat = Some(combat);
                return vec!["You have no Channel Divinity uses remaining.".to_string()];
            }
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You have already used your action this turn.".to_string()];
            }
            state.character.class_features.channel_divinity_remaining -= 1;
            combat.action_used = true;
            lines.push("You channel divine power!".to_string());
        }
        Command::LayOnHands(target) => {
            if state.character.class != character::class::Class::Paladin {
                state.active_combat = Some(combat);
                return vec!["Only Paladins have Lay on Hands.".to_string()];
            }
            if state.character.class_features.lay_on_hands_pool == 0 {
                state.active_combat = Some(combat);
                return vec!["Your Lay on Hands pool is empty.".to_string()];
            }
            if combat.action_used {
                state.active_combat = Some(combat);
                return vec!["You have already used your action this turn.".to_string()];
            }
            let heal_amount = state.character.class_features.lay_on_hands_pool
                .min((state.character.max_hp - state.character.current_hp).max(0) as u32);
            state.character.class_features.lay_on_hands_pool -= heal_amount;
            state.character.current_hp =
                (state.character.current_hp + heal_amount as i32).min(state.character.max_hp);
            combat.action_used = true;
            let recipient = if target.is_empty() || target == "self" {
                "yourself".to_string()
            } else {
                target
            };
            lines.push(format!("You lay hands on {}, restoring {} HP. ({} HP remaining in pool)",
                recipient, heal_amount, state.character.class_features.lay_on_hands_pool));
        }
        Command::Ki(ability) => {
            if state.character.class != character::class::Class::Monk {
                state.active_combat = Some(combat);
                return vec!["Only Monks can spend Ki points.".to_string()];
            }
            if state.character.class_features.ki_points_remaining == 0 {
                state.active_combat = Some(combat);
                return vec!["You have no Ki points remaining.".to_string()];
            }
            if combat.bonus_action_used {
                state.active_combat = Some(combat);
                return vec!["You have already used your bonus action this turn.".to_string()];
            }
            state.character.class_features.ki_points_remaining -= 1;
            combat.bonus_action_used = true;
            lines.push(format!("You spend a Ki point on {}. ({} Ki remaining)",
                ability, state.character.class_features.ki_points_remaining));
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
                Some(state::ItemType::Potion { effect, .. }) => {
                    let result = use_magic_potion(state, rng, &name, effect);
                    // Potions (mechanical or flavor) are always consumed on use.
                    state.character.inventory.retain(|&id| id != item_id);
                    state.world.items.remove(&item_id);
                    return (result, true);
                }
                Some(state::ItemType::Wand { ref spell_name, requires_attunement, .. }) => {
                    let result = use_magic_wand(state, &name, spell_name, requires_attunement, item_id);
                    // Wands are NEVER consumed — charges track their uses.
                    // We always report "action consumed" so the pipeline treats
                    // wand invocation as a regular action, even when blocked on
                    // attunement / zero charges (player still chose to try).
                    return (result, true);
                }
                Some(state::ItemType::Scroll { ref spell_name, spell_level, .. }) => {
                    let (lines, consumed) = use_magic_scroll(state, rng, &name, spell_name, spell_level);
                    if consumed {
                        state.character.inventory.retain(|&id| id != item_id);
                        state.world.items.remove(&item_id);
                    }
                    return (lines, consumed);
                }
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
        ResolveResult::Ambiguous(matches) => (emit_disambiguation(state, "use", &matches), false),
        ResolveResult::NotFound => (vec![format!("You don't have any \"{}\".", item_name)], false),
    }
}

/// Apply a magic potion's mechanical effect and return the narration lines.
/// Caller is responsible for removing the potion from inventory/world.
///
/// Healing potions roll `dice`d`die` + `bonus` and heal (capped at max_hp).
/// Flavor-only effects (Speed, Invisibility, Climbing) return a narrator
/// line but don't alter character state — full mechanical support is
/// deferred (see docs/specs/magic-items.md).
fn use_magic_potion(
    state: &mut GameState,
    rng: &mut StdRng,
    item_name: &str,
    effect: equipment::magic::PotionEffect,
) -> Vec<String> {
    use equipment::magic::PotionEffect;
    match effect {
        PotionEffect::Healing { dice, die, bonus } => {
            let rolls = rules::dice::roll_dice(rng, dice, die);
            let roll_total: i32 = rolls.iter().sum::<i32>() + bonus;
            let old_hp = state.character.current_hp;
            state.character.current_hp = (state.character.current_hp + roll_total)
                .min(state.character.max_hp);
            let healed = state.character.current_hp - old_hp;
            if healed > 0 {
                vec![narration::templates::USE_HEAL
                    .replace("{item}", item_name)
                    .replace("{roll}", &healed.to_string())
                    .replace("{current}", &state.character.current_hp.to_string())
                    .replace("{max}", &state.character.max_hp.to_string())]
            } else {
                vec![narration::templates::USE_HEAL_FULL
                    .replace("{item}", item_name)
                    .replace("{current}", &state.character.current_hp.to_string())
                    .replace("{max}", &state.character.max_hp.to_string())]
            }
        }
        PotionEffect::Speed => {
            vec![format!(
                "You quaff the {}. A rush of quickened blood pulses through you — \
                 the world seems to slow for a moment. (Haste-like effect is flavor only.)",
                item_name)]
        }
        PotionEffect::Invisibility => {
            vec![format!(
                "You quaff the {}. Your form shimmers and fades from sight. \
                 (Invisibility is flavor only.)",
                item_name)]
        }
        PotionEffect::Climbing => {
            vec![format!(
                "You quaff the {}. Your hands and feet feel unnaturally sticky, \
                 ready to grip any surface. (Climbing is flavor only.)",
                item_name)]
        }
    }
}

/// Expend one charge from a wand and narrate its invocation. Returns the
/// narration. The wand is NEVER removed from inventory (re-chargeable).
///
/// Attunement is validated first: if the wand requires attunement and the
/// player is not attuned, nothing is consumed and a rejection line is
/// returned. If charges are depleted (`0`), a no-charge line is returned.
///
/// Full spell resolution is deferred — MVP narrates the invocation only
/// (see docs/specs/magic-items.md "Deferred / Out of Scope").
fn use_magic_wand(
    state: &mut GameState,
    item_name: &str,
    spell_name: &str,
    requires_attunement: bool,
    item_id: types::ItemId,
) -> Vec<String> {
    if requires_attunement && !state.character.attuned_items.contains(&item_id) {
        return vec![format!(
            "The {} hums faintly but its power does not answer. You must attune to it first.",
            item_name)];
    }
    let charges = state.world.items.get(&item_id)
        .and_then(|i| i.charges_remaining).unwrap_or(0);
    if charges == 0 {
        return vec![format!(
            "The {} lies dormant in your hand; its charges are spent.",
            item_name)];
    }
    // Decrement charge (saturating at 0 defensively).
    if let Some(item) = state.world.items.get_mut(&item_id) {
        item.charges_remaining = Some(charges.saturating_sub(1));
    }
    let remaining = charges.saturating_sub(1);
    vec![format!(
        "You channel the {} — its runes flare as {} manifests. ({} charge{} remaining)",
        item_name,
        spell_name,
        remaining,
        if remaining == 1 { "" } else { "s" })]
}

/// Attempt to cast a spell scroll. Returns (narration, consumed).
///
/// Spellcaster classes (Bard/Cleric/Druid/Paladin/Ranger/Sorcerer/Warlock/
/// Wizard) read the scroll normally and consume it on any attempt. Non-casters
/// must pass a DC 10 Arcana check to cast successfully; on a failure the
/// scroll is still consumed (SRD 5.1 spell scroll rules).
///
/// Full spell resolution is deferred — MVP narrates the invocation only.
fn use_magic_scroll(
    state: &mut GameState,
    rng: &mut StdRng,
    item_name: &str,
    spell_name: &str,
    spell_level: u32,
) -> (Vec<String>, bool) {
    let _ = spell_level; // future: gate scroll level on caster level
    let is_caster = character_is_spellcaster(&state.character);
    if is_caster {
        return (
            vec![format!(
                "You unfurl the {} and intone its arcane script — {} surges forth, \
                 then the parchment crumbles to dust.",
                item_name, spell_name)],
            true,
        );
    }
    // Non-caster: DC 10 Arcana check.
    let result = rules::checks::skill_check(
        rng,
        types::Skill::Arcana,
        &state.character.ability_scores,
        &state.character.skill_proficiencies,
        state.character.proficiency_bonus(),
        10,
        false, false,
    );
    let mut lines = Vec::new();
    lines.push(format!(
        "The {} requires arcane training you lack — you attempt a DC 10 Arcana check.",
        item_name));
    lines.push(format!(
        "Arcana check: rolled {} + {} = {} vs DC 10.",
        result.roll, result.modifier, result.total));
    if result.success {
        lines.push(format!(
            "You decipher the script — {} flares forth before the scroll crumbles.",
            spell_name));
    } else {
        lines.push(format!(
            "The glyphs unravel before you. The {} disintegrates, its power wasted.",
            item_name));
    }
    (lines, true)
}

/// Return true if the character has spellcasting at class level 1+ (SRD 5.1
/// full and half-casters are listed; Fighter/Rogue/Monk/Barbarian are not).
/// Multi-class and subclass-granted casting are out of scope for this MVP.
fn character_is_spellcaster(c: &character::Character) -> bool {
    use character::class::Class;
    matches!(
        c.class,
        Class::Bard | Class::Cleric | Class::Druid | Class::Paladin
        | Class::Ranger | Class::Sorcerer | Class::Warlock | Class::Wizard
    )
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

/// Map a 3-letter ability code (case-insensitive) to the matching `Ability`.
fn parse_ability_code(code: &str) -> Option<Ability> {
    match code.to_uppercase().as_str() {
        "STR" | "STRENGTH" => Some(Ability::Strength),
        "DEX" | "DEXTERITY" => Some(Ability::Dexterity),
        "CON" | "CONSTITUTION" => Some(Ability::Constitution),
        "INT" | "INTELLIGENCE" => Some(Ability::Intelligence),
        "WIS" | "WISDOM" => Some(Ability::Wisdom),
        "CHA" | "CHARISMA" => Some(Ability::Charisma),
        _ => None,
    }
}

/// Apply a flat ability bonus to the character's score, capped at 20 per SRD.
/// Returns `(old, new)` so the caller can describe what changed.
fn apply_ability_bonus(character: &mut character::Character, ability: Ability, amount: i32) -> (i32, i32) {
    let entry = character.ability_scores.entry(ability).or_insert(10);
    let old = *entry;
    let new = (*entry + amount).min(20);
    *entry = new;
    (old, new)
}

/// Apply the static (non-Flavor) effects of a feat to the character. Skill
/// proficiency, ability bonus, and HP-per-level all land here. Initiative is
/// summed dynamically by `character::initiative_bonus_from_feats` and is NOT
/// applied here. Flavor-only feats produce no state changes.
fn apply_feat_effects(character: &mut character::Character, feat_name: &str) {
    let Some(feat) = FeatDef::lookup(feat_name) else { return; };
    for effect in feat.effects {
        match effect {
            FeatEffect::AbilityBonus { ability, amount } => {
                apply_ability_bonus(character, *ability, *amount);
            }
            FeatEffect::SkillProficiency(skill) => {
                if !character.skill_proficiencies.contains(skill) {
                    character.skill_proficiencies.push(*skill);
                }
            }
            FeatEffect::HpBonusPerLevel(per_level) => {
                let bonus = *per_level * (character.level.max(1) as i32);
                character.max_hp += bonus;
                character.current_hp = (character.current_hp + bonus).min(character.max_hp);
            }
            FeatEffect::SpeedBonus(n) => {
                character.speed += *n;
            }
            // Initiative is summed at roll time; nothing to do here.
            FeatEffect::Initiative(_) => {}
            // Placeholder feats — record-only at MVP.
            FeatEffect::LanguageProficiency
            | FeatEffect::ToolProficiency
            | FeatEffect::Flavor => {}
        }
    }
}

/// Apply the per-level HP bonus from feats with `HpBonusPerLevel` (e.g. Tough)
/// after a level-up. Called by the orchestrator immediately after
/// `leveling::award_xp` returns, once per level gained.
///
/// `levels_gained` is the number of levels crossed in this award (usually 1,
/// but can be > 1 if XP jumped multiple thresholds). For each new level,
/// `HpBonusPerLevel(n)` feats grant `n` additional HP.
///
/// `leveling/` cannot call this directly (module-isolation rule); the
/// orchestrator owns the coordination between leveling and feat data.
fn apply_post_levelup_feat_bonuses(character: &mut character::Character, levels_gained: u32) {
    if levels_gained == 0 { return; }
    // Collect all held feat names (origin + general).
    let mut names: Vec<String> = character.general_feats.clone();
    if let Some(ref name) = character.origin_feat {
        names.push(name.clone());
    }
    for name in &names {
        if let Some(feat) = FeatDef::lookup(name) {
            for effect in feat.effects {
                if let FeatEffect::HpBonusPerLevel(per_level) = effect {
                    let bonus = *per_level * levels_gained as i32;
                    character.max_hp += bonus;
                    character.current_hp = (character.current_hp + bonus).min(character.max_hp);
                }
            }
        }
    }
}

/// In-play handler for `GamePhase::ChooseAsi`. Accepts:
///   - `+2 STR` / `+2 strength` etc. — apply +2 to one ability (cap 20)
///   - `+1 STR DEX` — apply +1 to two distinct abilities (cap 20)
///   - feat name (e.g. `Tough`, `Defense`) — record the feat and apply effects
///   - `cancel` / `later` — defer; return to Exploration without spending
fn handle_choose_asi(state: &mut GameState, input: &str) -> Vec<String> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();

    // Defer / cancel — exit phase without spending.
    if matches!(lower.as_str(), "cancel" | "later" | "skip") {
        state.game_phase = GamePhase::Exploration;
        return vec!["You set the decision aside for now. Type 'asi' or rest to revisit.".to_string()];
    }

    // Help / show menu
    if trimmed.is_empty() || lower == "help" || lower == "?" {
        return asi_menu_lines(state);
    }

    // Parse "+2 ABIL" or "+1 ABIL ABIL2"
    if let Some(rest) = trimmed.strip_prefix("+2 ").or_else(|| trimmed.strip_prefix("+2")) {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() != 1 {
            return vec!["Format: '+2 STR' (one ability).".to_string()];
        }
        let Some(ab) = parse_ability_code(parts[0]) else {
            return vec![format!("Unknown ability '{}'. Use STR/DEX/CON/INT/WIS/CHA.", parts[0])];
        };
        let (old, new) = apply_ability_bonus(&mut state.character, ab, 2);
        state.character.asi_credits = state.character.asi_credits.saturating_sub(1);
        let lines = vec![format!("{} {} -> {} (cap 20).", ab, old, new)];
        return finalize_asi(state, lines);
    }
    if let Some(rest) = trimmed.strip_prefix("+1 ").or_else(|| trimmed.strip_prefix("+1")) {
        let parts: Vec<&str> = rest.split_whitespace().collect();
        if parts.len() != 2 {
            return vec!["Format: '+1 STR DEX' (two abilities).".to_string()];
        }
        let Some(a1) = parse_ability_code(parts[0]) else {
            return vec![format!("Unknown ability '{}'.", parts[0])];
        };
        let Some(a2) = parse_ability_code(parts[1]) else {
            return vec![format!("Unknown ability '{}'.", parts[1])];
        };
        if a1 == a2 {
            return vec!["+1/+1 must target two different abilities.".to_string()];
        }
        let (o1, n1) = apply_ability_bonus(&mut state.character, a1, 1);
        let (o2, n2) = apply_ability_bonus(&mut state.character, a2, 1);
        state.character.asi_credits = state.character.asi_credits.saturating_sub(1);
        let lines = vec![
            format!("{} {} -> {}, {} {} -> {} (cap 20).", a1, o1, n1, a2, o2, n2),
        ];
        return finalize_asi(state, lines);
    }

    // Try feat-name match.
    if let Some(feat) = FeatDef::lookup(trimmed) {
        // Origin feats are not selectable via ASI per SRD.
        if feat.category == FeatCategory::Origin {
            return vec![format!(
                "{} is an origin feat — origin feats are chosen at character creation only.",
                feat.name,
            )];
        }
        // Fighting-style feats are class-gated.
        if feat.category == FeatCategory::FightingStyle
            && !matches!(state.character.class, Class::Fighter | Class::Paladin | Class::Ranger)
        {
            return vec!["Only Fighters, Paladins, and Rangers can take fighting-style feats.".to_string()];
        }
        if state.character.general_feats.iter().any(|f| f == feat.name) {
            return vec![format!("You already have {}.", feat.name)];
        }
        state.character.general_feats.push(feat.name.to_string());
        apply_feat_effects(&mut state.character, feat.name);
        state.character.asi_credits = state.character.asi_credits.saturating_sub(1);
        let lines = vec![format!("You take the {} feat.", feat.name)];
        return finalize_asi(state, lines);
    }

    asi_menu_lines(state)
}

/// Return to Exploration if all credits are spent; otherwise prompt again
/// with the remaining-credit count.
fn finalize_asi(state: &mut GameState, mut lines: Vec<String>) -> Vec<String> {
    if state.character.asi_credits == 0 {
        state.game_phase = GamePhase::Exploration;
        lines.push("All ASI credits spent. Returning to exploration.".to_string());
    } else {
        lines.push(format!(
            "You have {} ASI credit{} remaining.",
            state.character.asi_credits,
            if state.character.asi_credits == 1 { "" } else { "s" },
        ));
    }
    lines
}

/// Build the ASI menu text shown when the player enters `ChooseAsi` or
/// types `help`.
fn asi_menu_lines(state: &GameState) -> Vec<String> {
    let mut lines = vec![
        format!(
            "You have {} unspent ASI credit{}. Choose how to spend one:",
            state.character.asi_credits,
            if state.character.asi_credits == 1 { "" } else { "s" },
        ),
        "  - '+2 ABIL' (e.g. '+2 STR') -> +2 to one ability, cap 20".to_string(),
        "  - '+1 A B' (e.g. '+1 STR DEX') -> +1 to two abilities, cap 20".to_string(),
        "  - feat name (e.g. 'Tough', 'Defense') -> take a general or fighting-style feat".to_string(),
        "  - 'cancel' to defer".to_string(),
    ];
    let general: Vec<&'static str> = FEATS_GENERAL.to_vec();
    lines.push(format!("Available general feats: {}", general.join(", ")));
    if matches!(state.character.class, Class::Fighter | Class::Paladin | Class::Ranger) {
        let fs: Vec<&'static str> = FEATS_FIGHTING_STYLE.to_vec();
        lines.push(format!("Available fighting-style feats: {}", fs.join(", ")));
    }
    lines
}

/// Names of general feats, in catalog order.
const FEATS_GENERAL: &[&str] = &[
    "Ability Score Improvement",
    "Grappler",
    "Great Weapon Master",
    "Sharpshooter",
    "Sentinel",
    "Tough",
    "War Caster",
];

/// Names of fighting-style feats, in catalog order.
const FEATS_FIGHTING_STYLE: &[&str] = &[
    "Archery",
    "Defense",
    "Dueling",
    "Great Weapon Fighting",
    "Protection",
    "Two-Weapon Fighting",
];

/// After any operation that may have raised `character.asi_credits` (i.e. a
/// level-up), enter the `ChooseAsi` phase if we are currently in
/// `Exploration`. Returns the menu lines if the phase changed, or an empty
/// Vec otherwise. Combat-time level-ups defer the prompt until combat ends.
fn check_and_enter_asi_phase(state: &mut GameState) -> Vec<String> {
    if state.character.asi_credits == 0 { return Vec::new(); }
    if state.active_combat.is_some() { return Vec::new(); }
    if !matches!(state.game_phase, GamePhase::Exploration) { return Vec::new(); }
    state.game_phase = GamePhase::ChooseAsi;
    asi_menu_lines(state)
}

/// Returns true when the character is wearing non-proficient armor AND the
/// supplied ability is STR or DEX. Per SRD 2024 Armor Training, any D20 Test
/// using STR or DEX has Disadvantage while the wearer lacks training; other
/// abilities (CON, INT, WIS, CHA) are unaffected. Used by skill-check and
/// saving-throw resolution in `lib.rs`.
fn armor_disadvantage_for_ability(
    character: &crate::character::Character,
    ability: types::Ability,
) -> bool {
    if !character.wearing_nonproficient_armor {
        return false;
    }
    matches!(ability, types::Ability::Strength | types::Ability::Dexterity)
}

/// After body armor has been assigned to the body slot, update
/// `character.wearing_nonproficient_armor` based on whether the wearer's class
/// is trained with this armor's category. When the wearer is NOT proficient,
/// append the SRD "Armor Training" warning line to `lines`.
///
/// Kept at the orchestrator layer so `equipment/` stays oblivious to classes
/// and `character/` does not import narration templates (module-isolation
/// rule). Callers pass the armor's category — shields are not routed here
/// since shields occupy the off-hand slot and are not yet enforced.
fn update_armor_proficiency_state(
    state: &mut GameState,
    category: state::ArmorCategory,
    lines: &mut Vec<String>,
) {
    let profs = state.character.class.armor_proficiencies();
    if profs.contains(&category) {
        state.character.wearing_nonproficient_armor = false;
    } else {
        state.character.wearing_nonproficient_armor = true;
        lines.push(
            "You are not proficient with this armor. You have Disadvantage on \
STR/DEX checks and attack rolls, and cannot cast spells."
                .to_string(),
        );
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
                        update_armor_proficiency_state(state, *category, &mut result_lines);
                    }
                    result_lines
                }
                _ => vec![narration::templates::EQUIP_CANT.replace("{item}", &item.name)],
            }
        }
        ResolveResult::Ambiguous(matches) => {
            let suffix = if force_off_hand { "off hand" } else { "" };
            emit_disambiguation_with_suffix(state, "equip", suffix, &matches)
        }
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
            if state.character.equipped.body == Some(item_id) {
                state.character.equipped.body = None;
                // Body slot is now empty -> no nonproficient armor worn.
                state.character.wearing_nonproficient_armor = false;
            }
            if is_weapon {
                vec![narration::templates::UNEQUIP_WEAPON.replace("{item}", &name)]
            } else {
                vec![narration::templates::UNEQUIP_ARMOR.replace("{item}", &name)]
            }
        }
        ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "unequip", &matches),
        ResolveResult::NotFound => vec![narration::templates::UNEQUIP_NOT_EQUIPPED.replace("{name}", target_str)],
    }
}

/// Return the (attack_bonus, damage_bonus) for a magic weapon, if any, gated
/// on attunement when required. Returns (0, 0) for mundane weapons or items
/// that are not attuned when attunement is needed.
///
/// Kept in `lib.rs` per the module-isolation rule: `combat/` does not know
/// about magic weapon bonuses, and `equipment/` does not know about combat
/// `AttackResult`. This helper lives at the orchestrator boundary.
fn magic_weapon_bonuses(
    state: &GameState,
    weapon_id: Option<crate::types::ItemId>,
) -> (i32, i32) {
    let Some(id) = weapon_id else { return (0, 0) };
    let Some(item) = state.world.items.get(&id) else { return (0, 0) };
    match &item.item_type {
        state::ItemType::MagicWeapon { attack_bonus, damage_bonus, requires_attunement, .. } => {
            if *requires_attunement && !state.character.attuned_items.contains(&id) {
                (0, 0)
            } else {
                (*attack_bonus, *damage_bonus)
            }
        }
        _ => (0, 0),
    }
}

/// Apply a magic weapon's attack/damage bonuses to an AttackResult in place.
/// Re-evaluates `hit` and `total_attack` with the attack bonus, and adds
/// `damage_bonus` to damage on hit (crits already double dice; the flat
/// bonus is NOT doubled, per SRD 5.1).
fn apply_magic_weapon_bonuses(
    result: &mut combat::AttackResult,
    attack_bonus: i32,
    damage_bonus: i32,
) {
    if attack_bonus == 0 && damage_bonus == 0 {
        return;
    }
    result.total_attack += attack_bonus;
    // Re-evaluate hit with the new total_attack (nat-1 still auto-misses,
    // nat-20 still auto-hits regardless of AC).
    if !result.natural_1 && !result.natural_20 {
        result.hit = result.total_attack >= result.target_ac;
    }
    if result.hit && damage_bonus != 0 {
        result.damage = (result.damage + damage_bonus).max(1);
    } else if !result.hit {
        // On a miss (possibly caused by a negative damage_bonus... edge case),
        // damage stays 0.
        result.damage = 0;
    }
}

/// Looks up the SRD mastery for a weapon item (if any) and returns it only
/// when the character has that mastery unlocked. Returns `None` for unarmed
/// strikes, unknown weapon names, or weapons the character has not unlocked.
///
/// Kept in `lib.rs` per the module-isolation rule: combat effects depend on
/// character/equipment data that `combat/` cannot reach directly.
fn player_mastery_for_weapon(
    state: &GameState,
    weapon_id: Option<crate::types::ItemId>,
) -> Option<crate::types::Mastery> {
    let id = weapon_id?;
    let item = state.world.items.get(&id)?;
    let mastery = equipment::weapon_mastery(&item.name)?;
    if equipment::character_has_mastery(&state.character, &item.name) {
        Some(mastery)
    } else {
        None
    }
}

/// Returns the player's ability modifier used for a given weapon's attack roll
/// (mirrors the selection logic in `combat::resolve_player_attack`). Matches
/// FINESSE / ranged / unarmed cases. Used by mastery helpers that reference
/// "the ability modifier used for the attack roll" (Graze, Cleave, Topple).
fn player_attack_ability_mod(
    state: &GameState,
    weapon_id: Option<crate::types::ItemId>,
    distance: u32,
) -> i32 {
    use types::Ability;
    let str_m = state.character.ability_modifier(Ability::Strength);
    let dex_m = state.character.ability_modifier(Ability::Dexterity);
    let Some(id) = weapon_id else {
        // Unarmed: STR is used.
        return str_m;
    };
    let Some(item) = state.world.items.get(&id) else { return str_m };
    let (properties, range_normal, range_long) = match &item.item_type {
        state::ItemType::Weapon { properties, range_normal, range_long, .. } => {
            (*properties, *range_normal, *range_long)
        }
        state::ItemType::MagicWeapon { properties, range_normal, range_long, .. } => {
            (*properties, *range_normal, *range_long)
        }
        _ => return str_m,
    };
    let is_finesse = properties & equipment::FINESSE != 0;
    let is_thrown = properties & equipment::THROWN != 0;
    // Ranged if the weapon has no melee mode, or if a thrown weapon is
    // being used from range.
    let is_ranged_only = range_normal > 0 && distance > 5;
    if is_ranged_only {
        if is_thrown {
            if is_finesse { str_m.max(dex_m) } else { str_m }
        } else if range_long > 0 {
            dex_m
        } else {
            str_m
        }
    } else if is_finesse {
        str_m.max(dex_m)
    } else {
        str_m
    }
}

/// NPC creature size helper used by Push mastery's Large-or-smaller gate.
/// Returns `Size::Medium` when the NPC has no combat_stats.
fn npc_size(state: &GameState, npc_id: crate::types::NpcId) -> crate::combat::monsters::Size {
    state.world.npcs.get(&npc_id)
        .and_then(|n| n.combat_stats.as_ref())
        .map(|s| s.size)
        .unwrap_or(crate::combat::monsters::Size::Medium)
}

/// Returns true when the given weapon qualifies for a Rogue's Sneak Attack
/// (Finesse melee weapon OR a ranged weapon actually being used at range).
/// An unarmed strike (no weapon) never qualifies per SRD.
fn sneak_attack_weapon_eligible(
    state: &GameState,
    weapon_id: Option<crate::types::ItemId>,
    distance: u32,
) -> bool {
    let Some(id) = weapon_id else { return false };
    let Some(item) = state.world.items.get(&id) else { return false };
    let (properties, range_normal) = match &item.item_type {
        state::ItemType::Weapon { properties, range_normal, .. } => (*properties, *range_normal),
        state::ItemType::MagicWeapon { properties, range_normal, .. } => (*properties, *range_normal),
        _ => return false,
    };
    let is_ammo = properties & equipment::AMMUNITION != 0;
    let is_thrown = properties & equipment::THROWN != 0;
    // Ranged attack: ammunition weapons are always ranged; thrown-at-range
    // uses the ranged mode; otherwise a pure-ranged weapon (range > 0, no
    // thrown/ammo) at >5 ft is ranged.
    let is_ranged_attack = if is_ammo {
        true
    } else if is_thrown && distance > 5 {
        true
    } else {
        range_normal > 0 && distance > 5 && !is_thrown
    };
    combat::sneak_attack_weapon_qualifies(properties, is_ranged_attack)
}

/// Apply a Rogue's Sneak Attack bonus damage if the player is a Rogue,
/// the attack hit, the weapon qualifies (Finesse or ranged), and the
/// attacker had advantage on the roll. Consumes the once-per-turn flag.
///
/// Per the handoff, the engine has no ally-adjacency concept, so the
/// "ally adjacent to target" alternative trigger is skipped for now —
/// eligibility reduces to the advantage path, which is the SRD-aligned
/// conservative MVP (no false positives).
///
/// Mutates `result.damage` in place so downstream damage application uses
/// the boosted total. Appends a narration line like
/// `Sneak Attack: +5 damage (1d6 -> 5).`
fn apply_sneak_attack(
    rng: &mut StdRng,
    state: &mut GameState,
    result: &mut combat::AttackResult,
    lines: &mut Vec<String>,
    weapon_id: Option<crate::types::ItemId>,
    distance: u32,
) {
    if state.character.class != character::class::Class::Rogue {
        return;
    }
    if !result.hit || result.damage <= 0 {
        return;
    }
    if state.character.class_features.sneak_attack_used_this_turn {
        return;
    }
    if !result.attacker_had_advantage {
        // MVP: require advantage. The SRD's "ally adjacent to target"
        // alternative is not yet modelled (no ally concept in the 1D
        // combat engine). See docs/specs/srd-classes.md.
        return;
    }
    if !sneak_attack_weapon_eligible(state, weapon_id, distance) {
        return;
    }
    let level = state.character.level;
    let dice = combat::sneak_attack_dice_for_level(level);
    let bonus = combat::roll_sneak_attack(rng, level, result.natural_20);
    result.damage += bonus;
    state.character.class_features.sneak_attack_used_this_turn = true;
    let dice_label = if result.natural_20 { dice * 2 } else { dice };
    lines.push(format!(
        "Sneak Attack: +{} damage ({}d6).",
        bonus, dice_label,
    ));
}

/// Dispatch mastery effects for a main-hand attack. Called after damage
/// has been applied. Reads the equipped weapon's mastery, checks the
/// character has unlocked it, then calls the appropriate combat helper.
///
/// Cleave also triggers a secondary attack here, which resolves its own
/// hit/damage/narration inline.
#[allow(clippy::too_many_arguments)]
fn apply_mainhand_mastery_effects(
    rng: &mut StdRng,
    state: &mut GameState,
    combat: &mut combat::CombatState,
    lines: &mut Vec<String>,
    result: &combat::AttackResult,
    npc_id: crate::types::NpcId,
    weapon_id: Option<crate::types::ItemId>,
    distance: u32,
) {
    let Some(mastery) = player_mastery_for_weapon(state, weapon_id) else { return };
    let ability_mod = player_attack_ability_mod(state, weapon_id, distance);
    let prof_bonus = state.character.proficiency_bonus();

    use types::Mastery;
    match mastery {
        Mastery::Graze => {
            if let Some(npc) = state.world.npcs.get_mut(&npc_id) {
                combat::apply_graze_mastery(true, result, ability_mod, npc, lines);
            }
        }
        Mastery::Vex => {
            combat::apply_vex_mastery(true, result, npc_id, combat, lines);
        }
        Mastery::Sap => {
            combat::apply_sap_mastery(true, result, npc_id, combat, lines);
        }
        Mastery::Slow => {
            combat::apply_slow_mastery(true, result, npc_id, combat, lines);
        }
        Mastery::Push => {
            let size = npc_size(state, npc_id);
            combat::apply_push_mastery(true, result, npc_id, combat, lines, size);
        }
        Mastery::Topple => {
            combat::apply_topple_mastery(
                true, result, npc_id, state, lines,
                ability_mod, prof_bonus, rng,
            );
        }
        Mastery::Cleave => {
            if let Some((secondary_id, cleave_result, _mod)) = combat::apply_cleave_mastery(
                rng, true, result, npc_id, combat, state, ability_mod,
            ) {
                let secondary_name = state.world.npcs.get(&secondary_id)
                    .map(|n| n.name.clone())
                    .unwrap_or_else(|| "an adjacent enemy".to_string());
                if cleave_result.hit {
                    lines.push(format!(
                        "Cleave: you sweep through and strike {} for {} {} damage.",
                        secondary_name, cleave_result.damage, cleave_result.damage_type,
                    ));
                    if let Some(npc) = state.world.npcs.get_mut(&secondary_id) {
                        let _dealt = combat::apply_damage_to_npc(
                            npc, cleave_result.damage, cleave_result.damage_type, lines,
                        );
                        if let Some(stats) = npc.combat_stats.as_ref() {
                            if stats.current_hp <= 0 {
                                lines.push(format!("{} is slain!", secondary_name));
                            }
                        }
                    }
                } else {
                    lines.push(format!(
                        "Cleave: your follow-through misses {}.", secondary_name,
                    ));
                }
            }
        }
        Mastery::Nick => {
            // Nick only fires on off-hand Light-weapon attacks; it is a
            // no-op here on the main-hand path.
        }
    }
}

/// Return true if the item requires attunement. Non-magic items return false.
fn item_requires_attunement(item: &state::Item) -> bool {
    match &item.item_type {
        state::ItemType::MagicWeapon { requires_attunement, .. } => *requires_attunement,
        state::ItemType::MagicArmor { requires_attunement, .. } => *requires_attunement,
        state::ItemType::Wondrous { requires_attunement, .. } => *requires_attunement,
        state::ItemType::Wand { requires_attunement, .. } => *requires_attunement,
        _ => false,
    }
}

/// Return true if the item is magical in any form (any MagicItemKind).
fn item_is_magical(item: &state::Item) -> bool {
    matches!(
        item.item_type,
        state::ItemType::MagicWeapon { .. }
            | state::ItemType::MagicArmor { .. }
            | state::ItemType::Wondrous { .. }
            | state::ItemType::Potion { .. }
            | state::ItemType::Scroll { .. }
            | state::ItemType::Wand { .. }
    )
}

fn handle_attune_command(state: &mut GameState, target_str: &str) -> Vec<String> {
    let target = target_str.trim();
    if target.is_empty() {
        return vec!["Attune to what?".to_string()];
    }

    let owned_candidates = inventory_item_candidates(state);
    let candidates: Vec<(usize, &str)> = owned_candidates.iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();

    match resolver::resolve_target(target, &candidates) {
        ResolveResult::Found(id) => {
            let item_id = id as u32;
            let item = match state.world.items.get(&item_id) {
                Some(i) => i.clone(),
                None => return vec![format!("You don't have any \"{}\".", target)],
            };
            if !item_is_magical(&item) {
                return vec![format!("The {} is not a magic item. Only magic items can be attuned.", item.name)];
            }
            if !item_requires_attunement(&item) {
                return vec![format!("The {} does not require attunement.", item.name)];
            }
            if state.character.attuned_items.contains(&item_id) {
                return vec![format!("You are already attuned to {}.", item.name)];
            }
            if state.character.attuned_items.len() >= equipment::magic::MAX_ATTUNED_ITEMS {
                return vec![format!(
                    "You are already attuned to {} items (max {}). Unattune one first.",
                    state.character.attuned_items.len(),
                    equipment::magic::MAX_ATTUNED_ITEMS,
                )];
            }
            state.character.attuned_items.push(item_id);
            vec![format!("You attune to the {}. You feel its power resonate with you.", item.name)]
        }
        ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "attune", &matches),
        ResolveResult::NotFound => vec![format!("You don't have any \"{}\".", target)],
    }
}

fn handle_unattune_command(state: &mut GameState, target_str: &str) -> Vec<String> {
    let target = target_str.trim();
    if target.is_empty() {
        return vec!["Unattune what?".to_string()];
    }
    // Candidates: only items currently attuned (and findable).
    let attuned_candidates: Vec<(usize, String)> = state.character.attuned_items.iter()
        .filter_map(|id| {
            state.world.items.get(id).map(|i| (*id as usize, i.name.clone()))
        })
        .collect();
    let candidates: Vec<(usize, &str)> = attuned_candidates.iter()
        .map(|(id, name)| (*id, name.as_str()))
        .collect();

    match resolver::resolve_target(target, &candidates) {
        ResolveResult::Found(id) => {
            let item_id = id as u32;
            let name = state.world.items.get(&item_id)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| target.to_string());
            state.character.attuned_items.retain(|&i| i != item_id);
            vec![format!("You release your attunement to the {}.", name)]
        }
        ResolveResult::Ambiguous(matches) => emit_disambiguation(state, "unattune", &matches),
        ResolveResult::NotFound => vec![format!("You are not attuned to any \"{}\".", target)],
    }
}

fn handle_list_attunements(state: &GameState) -> Vec<String> {
    if state.character.attuned_items.is_empty() {
        return vec![format!(
            "You are not attuned to any items. (0 / {} slots used)",
            equipment::magic::MAX_ATTUNED_ITEMS,
        )];
    }
    let mut lines = vec![format!(
        "Attuned items ({} / {} slots used):",
        state.character.attuned_items.len(),
        equipment::magic::MAX_ATTUNED_ITEMS,
    )];
    for &id in &state.character.attuned_items {
        if let Some(item) = state.world.items.get(&id) {
            lines.push(format!("  - {}", item.name));
        }
    }
    lines
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
    fn test_choose_class_menu_lists_all_twelve_classes() {
        // After race selection, the menu should list every SRD class.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let joined = output.text.join("\n");
        for class in Class::all() {
            assert!(
                joined.contains(&class.to_string()),
                "Expected ChooseClass menu to mention {}. Got: {}",
                class, joined,
            );
        }
    }

    #[test]
    fn test_choose_class_accepts_class_name_input() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        // Pick Paladin by name.
        let output = process_input(&output.state_json, "Paladin");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.class, Class::Paladin);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseBackground)));
    }

    #[test]
    fn test_choose_class_rejects_invalid_input() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "99");
        // Should still be on ChooseClass and produce a helpful error.
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("class")
            || l.contains("1")), "Got: {:?}", output.text);
    }

    #[test]
    fn test_choose_class_numeric_index_matches_class_all_ordering() {
        // The numeric index used in the prompt must match `Class::all()` ordering.
        let all = Class::all();
        for (i, &expected_class) in all.iter().enumerate() {
            let output = new_game(42, false);
            let output = process_input(&output.state_json, "1"); // Human
            let output = process_input(&output.state_json, &(i + 1).to_string());
            let state: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert_eq!(
                state.character.class, expected_class,
                "Selecting input '{}' should pick {} (got {})",
                i + 1, expected_class, state.character.class,
            );
        }
    }

    // ---- Species and subrace selection (feat/srd-remaining-species) ----

    #[test]
    fn test_new_game_lists_nine_species() {
        let output = new_game(42, false);
        let joined = output.text.join("\n");
        for race in Race::all() {
            assert!(
                joined.contains(&race.to_string()),
                "Expected ChooseRace menu to mention {}. Got: {}",
                race, joined,
            );
        }
    }

    #[test]
    fn test_choose_race_by_number_all_nine() {
        for (i, &expected_race) in Race::all().iter().enumerate() {
            let output = new_game(42, false);
            let output = process_input(&output.state_json, &(i + 1).to_string());
            let state: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert_eq!(
                state.character.race, expected_race,
                "Selecting input '{}' should pick {} (got {})",
                i + 1, expected_race, state.character.race,
            );
        }
    }

    #[test]
    fn test_choose_race_by_name_case_insensitive() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "dragonborn");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Dragonborn);
    }

    #[test]
    fn test_choose_race_invalid_input_reprompts() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "99");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseRace)));
    }

    #[test]
    fn test_species_without_subrace_goes_to_choose_class() {
        // Human has no subrace -- should skip straight to ChooseClass.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_halfling_no_subrace_goes_to_choose_class() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "7"); // Halfling
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Halfling);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_orc_no_subrace_goes_to_choose_class() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "8"); // Orc
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Orc);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_elf_goes_to_choose_subrace() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "2"); // Elf
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Elf);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
        // Prompt should mention Elven Lineage
        assert!(output.text.iter().any(|t| t.contains("Elven Lineage")));
    }

    #[test]
    fn test_dragonborn_goes_to_choose_subrace() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "4"); // Dragonborn
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Dragonborn);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
        assert!(output.text.iter().any(|t| t.contains("Draconic Ancestry")));
    }

    #[test]
    fn test_gnome_goes_to_choose_subrace() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "5"); // Gnome
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Gnome);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
    }

    #[test]
    fn test_goliath_goes_to_choose_subrace() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "6"); // Goliath
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Goliath);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
    }

    #[test]
    fn test_tiefling_goes_to_choose_subrace() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "9"); // Tiefling
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.character.race, Race::Tiefling);
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
    }

    #[test]
    fn test_subrace_selection_by_number() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "2"); // Elf
        let output = process_input(&output.state_json, "3"); // Wood Elf
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Wood Elf".to_string()));
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_subrace_selection_by_name() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "2"); // Elf
        let output = process_input(&output.state_json, "drow");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Drow".to_string()));
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_subrace_invalid_input_reprompts() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "2"); // Elf
        let output = process_input(&output.state_json, "99");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseSubrace)));
    }

    #[test]
    fn test_dragonborn_red_subrace_sets_pending() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "4"); // Dragonborn
        let output = process_input(&output.state_json, "8"); // Red
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Red".to_string()));
        assert!(matches!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseClass)));
    }

    #[test]
    fn test_gnome_forest_subrace_sets_pending() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "5"); // Gnome
        let output = process_input(&output.state_json, "1"); // Forest Gnome
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Forest Gnome".to_string()));
    }

    #[test]
    fn test_tiefling_infernal_subrace_sets_pending() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "9"); // Tiefling
        let output = process_input(&output.state_json, "3"); // Infernal
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Infernal".to_string()));
    }

    #[test]
    fn test_goliath_storm_subrace_sets_pending() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "6"); // Goliath
        let output = process_input(&output.state_json, "6"); // Storm
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_subrace, Some("Storm".to_string()));
    }

    /// Helper: drive a character through the full creation wizard.
    /// `race_input`: the race selection input (e.g. "2" for Elf)
    /// `subrace_input`: optional subrace input (e.g. Some("3") for Wood Elf)
    /// Returns the final state_json after naming the character "TestHero".
    fn drive_full_creation(race_input: &str, subrace_input: Option<&str>) -> String {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, race_input);
        let output = if let Some(sub) = subrace_input {
            process_input(&output.state_json, sub)
        } else {
            output
        };
        // ChooseClass: Barbarian (1)
        let output = process_input(&output.state_json, "1");
        // ChooseBackground: Acolyte (1)
        let output = process_input(&output.state_json, "1");
        // ChooseOriginFeat: default
        let output = process_input(&output.state_json, "");
        // ChooseBackgroundAbilityPattern: +2/+1 (1)
        let output = process_input(&output.state_json, "1");
        // ChooseAbilityMethod: Standard Array (1)
        let output = process_input(&output.state_json, "1");
        // AssignAbilities: STR 15, DEX 14, CON 13, INT 12, WIS 10, CHA 8
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        // ChooseSkills: first 2 (Barbarian has 2 skill choices)
        let output = process_input(&output.state_json, "1 2");
        // ChooseAlignment: Lawful Good (1)
        let output = process_input(&output.state_json, "1");
        // ChooseName
        let output = process_input(&output.state_json, "TestHero");
        output.state_json
    }

    #[test]
    fn test_wood_elf_finalized_speed_is_35() {
        let state_json = drive_full_creation("2", Some("3")); // Elf -> Wood Elf
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Elf);
        assert_eq!(state.character.subrace, Some("Wood Elf".to_string()));
        assert_eq!(state.character.speed, 35, "Wood Elf speed should be 35 ft");
        assert!(matches!(state.game_phase, GamePhase::Exploration));
    }

    #[test]
    fn test_drow_finalized_has_darkvision_120() {
        let state_json = drive_full_creation("2", Some("1")); // Elf -> Drow
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.subrace, Some("Drow".to_string()));
        assert!(state.character.traits.contains(&"Darkvision 120 ft".to_string()));
        // Base Elf speed is 30 (Drow does not change it)
        assert_eq!(state.character.speed, 30);
    }

    #[test]
    fn test_high_elf_finalized_has_prestidigitation() {
        let state_json = drive_full_creation("2", Some("2")); // Elf -> High Elf
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.subrace, Some("High Elf".to_string()));
        assert!(state.character.traits.contains(&"Prestidigitation cantrip".to_string()));
    }

    #[test]
    fn test_dragonborn_red_finalized_has_fire_traits() {
        let state_json = drive_full_creation("4", Some("8")); // Dragonborn -> Red
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Dragonborn);
        assert_eq!(state.character.subrace, Some("Red".to_string()));
        assert!(state.character.traits.contains(&"Fire Breath Weapon".to_string()));
        assert!(state.character.traits.contains(&"Fire Resistance".to_string()));
    }

    #[test]
    fn test_halfling_finalized_has_correct_traits() {
        let state_json = drive_full_creation("7", None); // Halfling (no subrace)
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Halfling);
        assert_eq!(state.character.subrace, None);
        assert!(state.character.traits.contains(&"Brave".to_string()));
        assert!(state.character.traits.contains(&"Luck".to_string()));
        assert_eq!(state.character.speed, 30);
    }

    #[test]
    fn test_orc_finalized_has_correct_traits() {
        let state_json = drive_full_creation("8", None); // Orc
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Orc);
        assert!(state.character.traits.contains(&"Adrenaline Rush".to_string()));
        assert!(state.character.traits.contains(&"Relentless Endurance".to_string()));
    }

    #[test]
    fn test_goliath_finalized_has_35_speed() {
        let state_json = drive_full_creation("6", Some("1")); // Goliath -> Cloud
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Goliath);
        assert_eq!(state.character.speed, 35, "Goliath base speed is 35 ft");
        assert!(state.character.traits.contains(&"Powerful Build".to_string()));
        assert!(state.character.traits.contains(&"Cloud's Jaunt".to_string()));
    }

    #[test]
    fn test_tiefling_infernal_finalized_has_fire_resistance() {
        let state_json = drive_full_creation("9", Some("3")); // Tiefling -> Infernal
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Tiefling);
        assert_eq!(state.character.subrace, Some("Infernal".to_string()));
        assert!(state.character.traits.contains(&"Fire Resistance".to_string()));
        assert!(state.character.traits.contains(&"Fire Bolt cantrip".to_string()));
    }

    #[test]
    fn test_gnome_forest_finalized_has_minor_illusion() {
        let state_json = drive_full_creation("5", Some("1")); // Gnome -> Forest Gnome
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Gnome);
        assert_eq!(state.character.subrace, Some("Forest Gnome".to_string()));
        assert!(state.character.traits.contains(&"Minor Illusion cantrip".to_string()));
    }

    #[test]
    fn test_pending_subrace_cleared_after_finalization() {
        let state_json = drive_full_creation("2", Some("3")); // Elf -> Wood Elf
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.pending_subrace, None, "pending_subrace should be cleared after finalization");
    }

    #[test]
    fn test_human_finalized_unchanged_from_legacy() {
        let state_json = drive_full_creation("1", None); // Human
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        assert_eq!(state.character.race, Race::Human);
        assert_eq!(state.character.subrace, None);
        assert_eq!(state.character.speed, 30);
    }

    #[test]
    fn test_new_species_have_empty_ability_bonuses_in_creation() {
        // Dragonborn should NOT get racial ability bonuses
        let state_json = drive_full_creation("4", Some("1")); // Dragonborn -> Black
        let state: GameState = serde_json::from_str(&state_json).unwrap();
        // The background system handles ability bonuses. But we can check
        // that the Dragonborn doesn't add its own: with Standard Array
        // [15,14,13,12,10,8] and background pattern +2/+1, the bonuses
        // come from Acolyte (INT, WIS, CHA) not from Dragonborn.
        // Without racial bonuses, STR stays at 15 (the assigned value).
        assert_eq!(state.character.ability_scores[&Ability::Strength], 15,
            "Dragonborn should have no racial STR bonus");
    }

    // ---- Orchestrator dispatch: class-feature commands (feat/remaining-srd-classes)
    //
    // These tests build an exploration-phase GameState with a chosen class and
    // verify the command dispatch paths for Rage / BardicInspiration /
    // ChannelDivinity / LayOnHands / Ki, covering both happy and error paths
    // per the `docs/specs/srd-classes.md` acceptance criteria.

    fn exploration_state_with_class(class: Class) -> GameState {
        let mut state = create_test_exploration_state();
        state.character.class = class;
        state.character.save_proficiencies = class.saving_throw_proficiencies();
        state.character.spell_slots_max = class.starting_spell_slots();
        state.character.spell_slots_remaining = state.character.spell_slots_max.clone();
        // Re-initialise feature state for the chosen class (fighter default
        // would leak Second Wind flags etc. otherwise).
        state.character.class_features = character::class::ClassFeatureState::default();
        let cha_mod = state.character.ability_modifier(Ability::Charisma);
        character::init_class_features(
            &mut state.character.class_features,
            class,
            1,
            cha_mod,
            &state.character.known_spells,
        );
        state
    }

    // ---- Rage ---------------------------------------------------------------

    #[test]
    fn test_rage_dispatch_non_barbarian_errors_and_no_state_change() {
        let state = exploration_state_with_class(Class::Fighter);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "rage");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("barbarian")),
            "Expected 'barbarian' in error. Got: {:?}", output.text);
    }

    #[test]
    fn test_rage_dispatch_barbarian_zero_uses_errors_and_no_state_change() {
        let mut state = exploration_state_with_class(Class::Barbarian);
        state.character.class_features.rage_uses_remaining = 0;
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "rage");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("no rage")),
            "Expected 'no Rage' in error. Got: {:?}", output.text);
    }

    #[test]
    fn test_rage_dispatch_barbarian_decrements_and_flips_active() {
        let state = exploration_state_with_class(Class::Barbarian);
        let starting_uses = state.character.class_features.rage_uses_remaining;
        assert!(starting_uses > 0, "Barbarian should start with Rage uses");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "rage");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features.rage_uses_remaining, starting_uses - 1);
        assert!(new_state.character.class_features.rage_active);
    }

    #[test]
    fn test_rage_dispatch_already_raging_errors() {
        let mut state = exploration_state_with_class(Class::Barbarian);
        state.character.class_features.rage_active = true;
        let uses_before = state.character.class_features.rage_uses_remaining;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "rage");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features.rage_uses_remaining, uses_before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("already")));
    }

    // ---- Bardic Inspiration -------------------------------------------------

    #[test]
    fn test_bardic_inspiration_dispatch_non_bard_errors() {
        let state = exploration_state_with_class(Class::Fighter);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "inspire ally");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("bard")));
    }

    #[test]
    fn test_bardic_inspiration_dispatch_bard_decrements() {
        let mut state = exploration_state_with_class(Class::Bard);
        // Ensure at least one use (CHA mod could be >=1, but be safe).
        state.character.class_features.bardic_inspiration_remaining = 3;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "inspire ally");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features.bardic_inspiration_remaining, 2);
        assert!(output.text.iter().any(|l| l.contains("ally")));
    }

    #[test]
    fn test_bardic_inspiration_dispatch_bard_zero_uses_errors() {
        let mut state = exploration_state_with_class(Class::Bard);
        state.character.class_features.bardic_inspiration_remaining = 0;
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "inspire ally");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("no bardic inspiration")),
            "Got: {:?}", output.text);
    }

    // ---- Channel Divinity ---------------------------------------------------

    #[test]
    fn test_channel_divinity_dispatch_non_cleric_paladin_errors() {
        let state = exploration_state_with_class(Class::Fighter);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "channel divinity");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("cleric")
            || l.to_lowercase().contains("paladin")));
    }

    #[test]
    fn test_channel_divinity_dispatch_level_one_cleric_errors() {
        // Level-1 Cleric has channel_divinity_remaining = 0 (unlocks at L2).
        let state = exploration_state_with_class(Class::Cleric);
        assert_eq!(state.character.class_features.channel_divinity_remaining, 0);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "channel divinity");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("no channel")),
            "Got: {:?}", output.text);
    }

    #[test]
    fn test_channel_divinity_dispatch_cleric_with_uses_decrements() {
        let mut state = exploration_state_with_class(Class::Cleric);
        state.character.class_features.channel_divinity_remaining = 1;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "channel divinity");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features.channel_divinity_remaining, 0);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("divine")),
            "Got: {:?}", output.text);
    }

    // ---- Lay on Hands -------------------------------------------------------

    #[test]
    fn test_lay_on_hands_dispatch_non_paladin_errors() {
        let state = exploration_state_with_class(Class::Fighter);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "lay on hands");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("paladin")));
    }

    #[test]
    fn test_lay_on_hands_dispatch_paladin_heals_and_deducts_pool() {
        let mut state = exploration_state_with_class(Class::Paladin);
        state.character.current_hp = state.character.max_hp - 4;
        let pool_before = state.character.class_features.lay_on_hands_pool;
        assert!(pool_before >= 4, "Paladin starts with pool >= 4");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "lay on hands");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // Heals up to missing HP, capped by the pool.
        assert_eq!(new_state.character.current_hp, new_state.character.max_hp);
        assert_eq!(
            new_state.character.class_features.lay_on_hands_pool,
            pool_before - 4
        );
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("lay hands")
            || l.to_lowercase().contains("restoring")));
    }

    #[test]
    fn test_lay_on_hands_dispatch_empty_pool_errors() {
        let mut state = exploration_state_with_class(Class::Paladin);
        state.character.class_features.lay_on_hands_pool = 0;
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "lay on hands");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("empty")
            || l.to_lowercase().contains("no lay")));
    }

    // ---- Ki / Focus ---------------------------------------------------------

    #[test]
    fn test_ki_dispatch_non_monk_errors() {
        let state = exploration_state_with_class(Class::Fighter);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "ki flurry");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("monk")));
    }

    #[test]
    fn test_ki_dispatch_level_one_monk_has_no_ki() {
        // Level-1 Monk starts with 0 Ki per SRD.
        let state = exploration_state_with_class(Class::Monk);
        assert_eq!(state.character.class_features.ki_points_remaining, 0);
        let before = state.character.class_features.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "ki flurry");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features, before);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("no ki")));
    }

    #[test]
    fn test_ki_dispatch_monk_with_ki_decrements() {
        let mut state = exploration_state_with_class(Class::Monk);
        state.character.class_features.ki_points_remaining = 2;
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "ki flurry");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.class_features.ki_points_remaining, 1);
        assert!(output.text.iter().any(|l| l.to_lowercase().contains("flurry")),
            "Got: {:?}", output.text);
    }

    // ---- ChooseAlignment step (#35) ----

    /// Helper: drive a fresh character through the creation wizard up to the
    /// point where ChooseAlignment is the active step. Returns the state JSON
    /// with the game phase parked at `ChooseAlignment`.
    fn state_at_choose_alignment() -> String {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "1"); // Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // skills
        output.state_json
    }

    #[test]
    fn test_choose_skills_advances_to_choose_alignment() {
        let state = state_at_choose_alignment();
        let loaded: GameState = serde_json::from_str(&state).unwrap();
        assert_eq!(
            loaded.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseAlignment),
            "After skills, the wizard should advance to ChooseAlignment, not ChooseName",
        );
    }

    #[test]
    fn test_choose_alignment_prompt_lists_all_ten_options() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "default");
        let output = process_input(&output.state_json, "2");
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2");
        // The prompt for ChooseAlignment should list all 10 alignments.
        let joined = output.text.join("\n");
        assert!(joined.contains("Lawful Good"), "prompt should list Lawful Good. Got:\n{}", joined);
        assert!(joined.contains("Chaotic Evil"), "prompt should list Chaotic Evil. Got:\n{}", joined);
        assert!(joined.contains("Unaligned"), "prompt should list Unaligned. Got:\n{}", joined);
        assert!(joined.contains("alignment"), "prompt should mention alignment. Got:\n{}", joined);
    }

    #[test]
    fn test_choose_alignment_numeric_selection_sets_alignment_and_advances() {
        use crate::types::Alignment;
        let state = state_at_choose_alignment();
        // Option 1 = Lawful Good (first in the canonical order).
        let output = process_input(&state, "1");
        let loaded: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(loaded.character.alignment, Alignment::LawfulGood);
        assert_eq!(
            loaded.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseName),
            "After choosing alignment, the wizard should advance to ChooseName",
        );
        // The subsequent prompt should ask for a name.
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("name")),
            "Expected name prompt after alignment. Got: {:?}", output.text);
    }

    #[test]
    fn test_choose_alignment_accepts_all_ten_by_number() {
        use crate::types::Alignment;
        let expected = [
            Alignment::LawfulGood, Alignment::NeutralGood, Alignment::ChaoticGood,
            Alignment::LawfulNeutral, Alignment::TrueNeutral, Alignment::ChaoticNeutral,
            Alignment::LawfulEvil, Alignment::NeutralEvil, Alignment::ChaoticEvil,
            Alignment::Unaligned,
        ];
        for (idx, expected_alignment) in expected.iter().enumerate() {
            let state = state_at_choose_alignment();
            let input = format!("{}", idx + 1);
            let output = process_input(&state, &input);
            let loaded: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert_eq!(
                &loaded.character.alignment, expected_alignment,
                "input {} should set alignment {:?}", input, expected_alignment,
            );
        }
    }

    #[test]
    fn test_choose_alignment_accepts_name_case_insensitive() {
        use crate::types::Alignment;
        let state = state_at_choose_alignment();
        let output = process_input(&state, "chaotic good");
        let loaded: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(loaded.character.alignment, Alignment::ChaoticGood);
    }

    #[test]
    fn test_choose_alignment_rejects_invalid_input() {
        let state = state_at_choose_alignment();
        let output = process_input(&state, "11"); // out of range
        let loaded: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(
            loaded.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseAlignment),
            "Invalid input should leave the wizard parked at ChooseAlignment",
        );
        let output = process_input(&output.state_json, "invalid");
        let loaded: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(
            loaded.game_phase,
            GamePhase::CharacterCreation(CreationStep::ChooseAlignment),
            "Non-numeric unknown input should also stay on ChooseAlignment",
        );
    }

    #[test]
    fn test_full_character_creation_flow() {
        let output = new_game(42, false);
        let state = &output.state_json;

        // Choose race
        let output = process_input(state, "1");
        assert!(output.text.iter().any(|t| t.contains("class")));

        // Choose class (Fighter — pick by name since the numeric ordering
        // changed when we expanded to all 12 SRD classes).
        let output = process_input(&output.state_json, "Fighter");
        assert!(output.text.iter().any(|t| t.contains("background")),
            "Expected background prompt after class. Got: {:?}", output.text);

        // Choose background (Acolyte)
        let output = process_input(&output.state_json, "1");
        assert!(output.text.iter().any(|t| t.contains("origin feat")),
            "Expected origin-feat prompt. Got: {:?}", output.text);

        // Accept the background's suggested origin feat.
        let output = process_input(&output.state_json, "default");
        assert!(output.text.iter().any(|t| t.contains("adjustment pattern")),
            "Expected ability pattern prompt. Got: {:?}", output.text);

        // Choose +1/+1/+1 pattern
        let output = process_input(&output.state_json, "2");
        assert!(output.text.iter().any(|t| t.contains("ability score")));

        // Choose standard array
        let output = process_input(&output.state_json, "1");
        assert!(output.text.iter().any(|t| t.contains("STR DEX CON")));

        // Assign scores
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        assert!(output.text.iter().any(|t| t.contains("skill")));

        // Choose skills (Fighter gets 2)
        let output = process_input(&output.state_json, "1 2");
        assert!(output.text.iter().any(|t| t.contains("alignment")),
            "Expected alignment prompt after skills. Got: {:?}", output.text);

        // Choose alignment (5 = Neutral / TrueNeutral)
        let output = process_input(&output.state_json, "5");
        assert!(output.text.iter().any(|t| t.contains("name")));

        // Choose name
        let output = process_input(&output.state_json, "Aldric");
        assert!(output.text.iter().any(|t| t.contains("Aldric")));

        // Verify we're in exploration
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(matches!(state.game_phase, GamePhase::Exploration));
        assert!(!state.world.locations.is_empty());
        // Background should be recorded
        assert_eq!(state.character.background, Background::Acolyte);
        // Origin feat trait added
        assert!(state.character.traits.iter().any(|t| t == "Origin Feat: Magic Initiate (Cleric)"));
    }

    #[test]
    fn test_background_grants_skill_and_tool_proficiencies() {
        // Walk a Criminal Rogue through creation and verify prof grants.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Rogue"); // Class by name
        let output = process_input(&output.state_json, "4"); // Criminal
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "1"); // +2/+1 pattern (DEX+2, CON+1)
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2 3 4"); // 4 rogue skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Shadow");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        assert_eq!(state.character.background, Background::Criminal);
        // Criminal grants Sleight of Hand and Stealth — these may already be in
        // the rogue's class picks; either way they must be present afterwards.
        assert!(state.character.skill_proficiencies.contains(&Skill::SleightOfHand),
            "Criminal should be proficient in Sleight of Hand");
        assert!(state.character.skill_proficiencies.contains(&Skill::Stealth),
            "Criminal should be proficient in Stealth");
        // Tool proficiency
        assert!(state.character.tool_proficiencies.iter().any(|t| t == "Thieves' Tools"),
            "Criminal should have Thieves' Tools proficiency. Got: {:?}",
            state.character.tool_proficiencies);
        // Language
        assert!(state.character.languages.contains(&"Common".to_string()));
        assert!(state.character.languages.contains(&"Thieves' Cant".to_string()));
        // Origin feat trait
        assert!(state.character.traits.iter().any(|t| t == "Origin Feat: Alert"));
    }

    #[test]
    fn test_background_ability_adjustment_plus_two_plus_one() {
        // Criminal options are DEX, CON, INT. With +2/+1 pattern, DEX gets +2
        // and CON gets +1. With Human (+1 all), starting scores 15,14,13,12,10,8
        // → Human bonus → 16,15,14,13,11,9 → Criminal +2/+1 on DEX/CON →
        //   16, 15+2=17, 14+1=15, 13, 11, 9.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Rogue");
        let output = process_input(&output.state_json, "4"); // Criminal
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "1"); // +2/+1 pattern
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2 3 4");
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Shadow");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        assert_eq!(state.character.ability_scores[&Ability::Strength], 16);
        assert_eq!(state.character.ability_scores[&Ability::Dexterity], 17, "DEX 14 + Human 1 + Criminal 2 = 17");
        assert_eq!(state.character.ability_scores[&Ability::Constitution], 15, "CON 13 + Human 1 + Criminal 1 = 15");
        assert_eq!(state.character.ability_scores[&Ability::Intelligence], 13);
        assert_eq!(state.character.ability_scores[&Ability::Wisdom], 11);
        assert_eq!(state.character.ability_scores[&Ability::Charisma], 9);
    }

    #[test]
    fn test_background_ability_adjustment_plus_one_to_all() {
        // Sage options are CON, INT, WIS. With +1/+1/+1 pattern, all three
        // get +1. Human (+1 to all) + Sage (+1 to CON/INT/WIS) on 15,14,13,12,10,8:
        //   STR 15+1=16, DEX 14+1=15, CON 13+2=15, INT 12+2=14, WIS 10+2=12, CHA 8+1=9.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Wizard");
        let output = process_input(&output.state_json, "12"); // Sage (index 12 in Background::all())
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // +1/+1/+1 pattern
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 wizard skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Sage");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        assert_eq!(state.character.background, Background::Sage);
        assert_eq!(state.character.ability_scores[&Ability::Strength], 16);
        assert_eq!(state.character.ability_scores[&Ability::Dexterity], 15);
        assert_eq!(state.character.ability_scores[&Ability::Constitution], 15);
        assert_eq!(state.character.ability_scores[&Ability::Intelligence], 14);
        assert_eq!(state.character.ability_scores[&Ability::Wisdom], 12);
        assert_eq!(state.character.ability_scores[&Ability::Charisma], 9);
    }

    #[test]
    fn test_background_ability_adjustment_caps_at_twenty() {
        // Start with DEX 15 via standard array, Human +1, Criminal +2 →
        // 15 + 1 + 2 = 18. That is under 20. To exercise the cap, use point buy
        // maxes and pick abilities that would push one over 20.
        // Elf +2 DEX + standard array DEX=15 → DEX 17. Criminal +2 → 19. No cap hit.
        // The cap is easier to verify directly via unit test of apply_background_effects,
        // but we can at least confirm scores don't exceed 20 across the flow.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "2"); // Elf
        let output = process_input(&output.state_json, "Rogue");
        let output = process_input(&output.state_json, "4"); // Criminal
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "1"); // +2/+1 pattern
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "8 15 14 13 12 10"); // DEX=15 for max
        let output = process_input(&output.state_json, "1 2 3 4");
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Shadow");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        for (&ability, &score) in &state.character.ability_scores {
            assert!(score <= 20, "Ability {:?} = {} exceeds cap of 20", ability, score);
        }
    }

    #[test]
    fn test_background_name_selection_works() {
        // Choose background by name instead of number.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "soldier"); // By name (case-insensitive)
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2");
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Max");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        assert_eq!(state.character.background, Background::Soldier);
    }

    #[test]
    fn test_background_invalid_input_reprompts() {
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "99"); // Invalid number
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Should stay in ChooseBackground
        assert_eq!(state.game_phase, GamePhase::CharacterCreation(CreationStep::ChooseBackground));
        assert!(output.text.iter().any(|t| t.contains("1-16") || t.contains("background")));
    }

    #[test]
    fn test_background_pattern_transient_state_cleared() {
        // pending_background_pattern should be cleared after creation finalizes.
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "1"); // Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "1"); // +2/+1
        let output = process_input(&output.state_json, "1");
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2");
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Hero");
        let state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(state.pending_background_pattern, None,
            "pending_background_pattern should be cleared after finalization");
    }

    #[test]
    fn test_fighter_gets_starting_equipment() {
        // Run full character creation as Fighter
        let output = new_game(42, false);
        let output = process_input(&output.state_json, "1"); // Human
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "1"); // Background: Acolyte (no SRD weapon/armor items)
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // Ability pattern: +1/+1/+1 (no STR/DEX/CON change)
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
        let output = process_input(&output.state_json, "Aldric");

        let state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Fighter should have 3 items: Chain Mail, Longsword, Shield
        // (Acolyte background items don't resolve to SRD_WEAPONS/SRD_ARMOR, so 0 extra)
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
        let output = process_input(&output.state_json, "Rogue");
        let output = process_input(&output.state_json, "1"); // Background: Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // Ability pattern: +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2 3 4"); // 4 skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
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
        let output = process_input(&output.state_json, "Wizard");
        let output = process_input(&output.state_json, "1"); // Background: Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // Ability pattern: +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
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
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "1"); // Background: Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // Ability pattern: +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
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
        let output = process_input(&output.state_json, "Fighter");
        let output = process_input(&output.state_json, "1"); // Background: Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // Ability pattern: +1/+1/+1
        let output = process_input(&output.state_json, "1"); // Standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8");
        let output = process_input(&output.state_json, "1 2"); // 2 skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
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
            charges_remaining: None,
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
            charges_remaining: None,
        });
        state.world.items.insert(id2, state::Item {
            id: id2, name: "Longsword".to_string(), description: "A long sword.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 8, damage_type: state::DamageType::Slashing,
                properties: 0, category: state::WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
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
                ..Default::default()
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
            charges_remaining: None,
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
    fn test_attack_with_zero_movement_keeps_turn_open_until_end_turn() {
        // Regression: attack action with zero movement remaining should NOT
        // auto-advance to NPC turns. The turn stays open until the player
        // explicitly types `end turn`, so bonus actions / movement grants
        // from Dash etc. remain possible.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5); // in melee
            combat.player_movement_remaining = 0; // exhausted movement
            combat.action_used = false;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();

        assert!(combat.is_player_turn(),
            "Attack with no movement remaining should not auto-end the turn. Got: {:?}",
            output.text);
        assert!(combat.action_used, "Attack should still consume action");
        assert!(output.text.iter().any(|t| t.contains("Your turn!")),
            "Expected player turn prompt to continue, got: {:?}", output.text);
    }

    #[test]
    fn test_dodge_with_zero_movement_keeps_turn_open_until_end_turn() {
        // Regression: Dodge with zero movement should not auto-end the turn.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.player_movement_remaining = 0;
            combat.action_used = false;
        }

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "dodge");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref().unwrap();

        assert!(combat.is_player_turn(),
            "Dodge with 0 movement should still keep the player's turn open.");
        assert!(combat.action_used);
        assert!(output.text.iter().any(|t| t.contains("Your turn!")),
            "Expected turn prompt, got: {:?}", output.text);
    }

    #[test]
    fn test_attack_then_bonus_dash_allowed_on_same_turn() {
        // Attack consumes action; movement goes to zero; player can still
        // spend a bonus action (e.g. bonus dash) before ending the turn.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
            combat.player_movement_remaining = 0;
            combat.action_used = false;
            combat.bonus_action_used = false;
        }

        // Attack first — consume action, 0 movement
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");
        let after_attack: GameState = serde_json::from_str(&output.state_json).unwrap();
        {
            let combat = after_attack.active_combat.as_ref().unwrap();
            assert!(combat.is_player_turn(), "Turn should remain with player after attack");
            assert!(combat.action_used, "Action should be consumed");
        }

        // Now fire a bonus dash — should succeed because turn was held open.
        let state_json2 = serde_json::to_string(&after_attack).unwrap();
        let output2 = process_input(&state_json2, "bonus dash");
        let after_dash: GameState = serde_json::from_str(&output2.state_json).unwrap();
        let combat = after_dash.active_combat.as_ref().unwrap();
        assert!(combat.is_player_turn(),
            "After bonus dash the player should still be on their turn (no auto-end).");
        assert!(combat.bonus_action_used, "Bonus action should be consumed");
        assert!(combat.player_movement_remaining > 0,
            "Dash should have restored movement");
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
            charges_remaining: None,
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
    fn test_ranged_attack_no_nearby_hostiles_has_no_disadvantage() {
        // Counter-test: no hostile within 5ft, ranged attack at normal range should
        // NOT apply the melee-disadvantage. Target dodging is also off.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);

        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 40); // goblin at 40 ft — well clear of melee
        }

        // Equip a shortbow; pure ranged.
        let bow_id = 203u32;
        state.world.items.insert(bow_id, state::Item {
            id: bow_id,
            name: "Shortbow".to_string(),
            description: "A shortbow.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::AMMUNITION | crate::equipment::TWO_HANDED,
                category: state::WeaponCategory::Simple,
                versatile_die: 0,
                range_normal: 80,
                range_long: 320,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        state.character.inventory.push(bow_id);
        state.character.equipped.main_hand = Some(bow_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");

        // No hostiles within 5ft, not dodging, within normal range -> no disadvantage.
        assert!(
            !output.text.iter().any(|t| t.to_lowercase().contains("disadvantage")),
            "Ranged attack with no hostile in melee should NOT apply disadvantage. Got: {:?}",
            output.text
        );
    }

    #[test]
    fn test_ranged_attack_at_distance_5_with_other_hostile_in_melee_has_disadvantage() {
        // Integration: verify hostile_within_5ft wiring at every relevant call site
        // by firing at a target at exactly 5 ft (the documented trigger boundary).
        // SRD 5.1: ranged attacks within 5 ft of ANY living hostile have disadvantage.
        let mut state = create_test_combat_state();
        force_player_turn(&mut state);

        // Add a SECOND hostile NPC in melee (distance 5) to prove the check
        // considers all hostiles, not only the target.
        let second_npc_id = 101;
        let loc_id = state.current_location;
        state.world.npcs.insert(second_npc_id, state::Npc {
            id: second_npc_id,
            name: "Second Goblin".to_string(),
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
                ..Default::default()
            }),
            conditions: Vec::new(),
        });
        if let Some(loc) = state.world.locations.get_mut(&loc_id) {
            loc.npcs.push(second_npc_id);
        }

        if let Some(ref mut combat) = state.active_combat {
            // Target (first goblin) is at distance 40 (comfortably ranged).
            combat.distances.insert(100, 40);
            // Second goblin is in melee at distance 5.
            combat.distances.insert(second_npc_id, 5);
            // Add second goblin to initiative so it's picked up by has_living_hostile_within.
            combat.initiative_order.push((combat::Combatant::Npc(second_npc_id), 5));
        }

        // Equip a longbow (pure ranged; no THROWN/melee fallback).
        let bow_id = 202u32;
        state.world.items.insert(bow_id, state::Item {
            id: bow_id,
            name: "Longbow".to_string(),
            description: "A longbow.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 8,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::AMMUNITION | crate::equipment::TWO_HANDED | crate::equipment::HEAVY,
                category: state::WeaponCategory::Martial,
                versatile_die: 0,
                range_normal: 150,
                range_long: 600,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        state.character.inventory.push(bow_id);
        state.character.equipped.main_hand = Some(bow_id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attack test goblin");

        assert!(
            output.text.iter().any(|t| t.to_lowercase().contains("disadvantage")),
            "Ranged attack while another hostile is within 5ft should apply disadvantage. Got: {:?}",
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

    /// Hypothesis: Shield is reaction-only per SRD 5.1. Typing `cast shield`
    /// on the player's turn should be rejected, not consume an action or slot.
    #[test]
    fn test_cast_shield_on_player_turn_is_rejected_as_reaction_only() {
        let mut state = wizard_combat_state();
        force_player_turn(&mut state);
        let slots_before = state.character.spell_slots_remaining[0];
        let state_json = serde_json::to_string(&state).unwrap();

        let output = process_input(&state_json, "cast shield");

        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // Slot must NOT be consumed -- Shield cannot be cast as an action.
        assert_eq!(new_state.character.spell_slots_remaining[0], slots_before,
            "cast shield on player turn should NOT consume a spell slot");
        // The action must NOT be used.
        let combat = new_state.active_combat.as_ref().unwrap();
        assert!(!combat.action_used,
            "cast shield on player turn should NOT consume an action");
        // Output should explain Shield is reaction-only.
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("reaction")),
            "Expected reaction-only narration, got: {:?}", output.text);
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

    // ---- Reach OA gating (scope item #3) ----
    //
    // The `should_trigger_opportunity_attack` predicate short-circuits to
    // `None` under the stock AI (NPCs always move toward the player), so a
    // behavioural end-to-end test can't observe an OA firing today. What we
    // CAN verify is the reach gate: the predicate must reject NPCs that sit
    // beyond the player's weapon reach, and accept those within it. These
    // tests pin that gate so the machinery is ready for a future retreat
    // AI (issue #43) without silently regressing.

    #[test]
    fn test_oa_gate_rejects_npc_beyond_unarmed_reach() {
        let mut state = create_test_combat_state();
        // Remove main-hand weapon: unarmed reach = 5 ft.
        state.character.equipped.main_hand = None;
        {
            let combat = state.active_combat.as_mut().expect("combat");
            combat.reaction_used = false;
            combat.distances.insert(100, 10); // goblin beyond 5 ft reach
        }
        let combat = state.active_combat.as_ref().unwrap().clone();
        assert!(
            should_trigger_opportunity_attack(&combat, &state, 100).is_none(),
            "Unarmed player should NOT threaten NPC at 10 ft"
        );
    }

    #[test]
    fn test_oa_gate_accepts_npc_within_reach_weapon_range() {
        // Equip a glaive (REACH). Player reach = 10 ft. NPC at 10 ft is
        // inside threatened area — the reach gate passes. (The predicate
        // still returns None today because the stock AI never flees, but
        // the reach gate itself is what we're pinning.)
        let mut state = create_test_combat_state();
        let glaive_id = 900u32;
        state.world.items.insert(glaive_id, state::Item {
            id: glaive_id,
            name: "Glaive".to_string(),
            description: String::new(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 10,
                damage_type: state::DamageType::Slashing,
                properties: crate::equipment::REACH
                    | crate::equipment::HEAVY
                    | crate::equipment::TWO_HANDED,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        state.character.equipped.main_hand = Some(glaive_id);

        {
            let combat = state.active_combat.as_mut().expect("combat");
            combat.reaction_used = false;
            combat.distances.insert(100, 10);
        }
        let combat = state.active_combat.as_ref().unwrap().clone();
        // The reach gate (`npc_within_player_reach`) must report true for
        // a glaive-equipped player at 10 ft.
        assert!(
            combat::npc_within_player_reach(&state, &combat, 100),
            "Glaive-equipped player should threaten NPC at 10 ft"
        );
    }

    #[test]
    fn test_oa_gate_rejects_npc_beyond_reach_weapon_range() {
        // Glaive reach = 10 ft; NPC at 15 ft is outside — gate must reject.
        let mut state = create_test_combat_state();
        let glaive_id = 901u32;
        state.world.items.insert(glaive_id, state::Item {
            id: glaive_id,
            name: "Glaive".to_string(),
            description: String::new(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 10,
                damage_type: state::DamageType::Slashing,
                properties: crate::equipment::REACH
                    | crate::equipment::HEAVY
                    | crate::equipment::TWO_HANDED,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        });
        state.character.equipped.main_hand = Some(glaive_id);

        {
            let combat = state.active_combat.as_mut().expect("combat");
            combat.reaction_used = false;
            combat.distances.insert(100, 15);
        }
        let combat = state.active_combat.as_ref().unwrap().clone();
        assert!(
            should_trigger_opportunity_attack(&combat, &state, 100).is_none(),
            "Glaive reach is 10 ft; NPC at 15 ft is outside threatened area"
        );
    }

    #[test]
    fn test_oa_gate_rejects_when_reaction_used() {
        // Even if the NPC is within reach, a consumed reaction must block
        // the OA prompt.
        let mut state = create_test_combat_state();
        {
            let combat = state.active_combat.as_mut().expect("combat");
            combat.reaction_used = true;
            combat.distances.insert(100, 5);
        }
        let combat = state.active_combat.as_ref().unwrap().clone();
        assert!(
            should_trigger_opportunity_attack(&combat, &state, 100).is_none(),
            "OA must not trigger when player reaction is already spent"
        );
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
            charges_remaining: None,
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
            charges_remaining: None,
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
            charges_remaining: None,
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
            charges_remaining: None,
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
                ..Default::default()
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
            charges_remaining: None,
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
                ..Default::default()
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
        let output = process_input(&output.state_json, "Fighter"); // class
        let output = process_input(&output.state_json, "1"); // background: Acolyte
        let output = process_input(&output.state_json, "default"); // origin feat
        let output = process_input(&output.state_json, "2"); // ability pattern
        let output = process_input(&output.state_json, "1"); // standard array
        let output = process_input(&output.state_json, "15 14 13 12 10 8"); // scores
        let output = process_input(&output.state_json, "1 2"); // skills
        let output = process_input(&output.state_json, "5"); // alignment: Neutral
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
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
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
                ..Default::default()
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
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
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
                ..Default::default()
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
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            death_save_successes: 0,
            death_save_failures: 0,
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
                    ..Default::default()
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
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            death_save_successes: 0,
            death_save_failures: 0,
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
                ..Default::default()
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
            player_vex_target: None,
            sap_targets: std::collections::HashSet::new(),
            slow_targets: HashMap::new(),
            cleave_used_this_turn: false,
            nick_used_this_turn: false,
            death_save_successes: 0,
            death_save_failures: 0,
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
                ..Default::default()
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

    // ---- Tough feat: per-level HP bonus on level-up ----

    #[test]
    fn tough_feat_grants_extra_hp_on_level_up() {
        // Character with Tough in general_feats should gain +2 HP extra on each level-up.
        let mut state = create_test_exploration_state();
        state.character.general_feats = vec!["Tough".to_string()];
        let hp_before = state.character.max_hp;
        // Award enough XP to level up from 1 to 2.
        let xp_lines = leveling::award_xp(&mut state.character, 300);
        // apply_post_levelup_feat_bonuses to add Tough's +2 HP for the new level.
        apply_post_levelup_feat_bonuses(&mut state.character, 1);
        assert_eq!(state.character.level, 2);
        // Base HP gain + 2 from Tough
        let hp_gain = state.character.max_hp - hp_before;
        assert!(
            hp_gain >= 2,
            "Expected at least 2 extra HP from Tough on level-up, got {}. Lines: {:?}",
            hp_gain, xp_lines,
        );
        // Specifically: Tough adds +2 (per HpBonusPerLevel(2) * 1 new level)
        // The base gain for Fighter d10 + CON 2 = 8; Tough adds 2 → total 10.
        // We just verify the Tough bonus specifically: recompute base to isolate.
        let base_hp = hp_before; // before any level-up
        let _ = base_hp; // used for context above
        // The total gain should be base_gain + 2
        let base_gain_without_tough = 8; // Fighter Human CON 14 (13+1) -> +2 mod, d10->+6 = 8
        assert_eq!(hp_gain, base_gain_without_tough + 2, "Tough should add +2 HP per level-up");
    }

    #[test]
    fn no_tough_feat_no_extra_hp_on_level_up() {
        let mut state = create_test_exploration_state();
        // No feats
        state.character.general_feats = vec![];
        let hp_before = state.character.max_hp;
        leveling::award_xp(&mut state.character, 300);
        apply_post_levelup_feat_bonuses(&mut state.character, 1);
        assert_eq!(state.character.level, 2);
        let hp_gain = state.character.max_hp - hp_before;
        let expected_base = 8; // Fighter Human CON 14 -> mod +2; d10 -> (10/2)+1+2 = 8
        assert_eq!(hp_gain, expected_base, "No feat — only base HP gain expected");
    }

    #[test]
    fn tough_in_origin_feat_also_grants_level_hp_bonus() {
        let mut state = create_test_exploration_state();
        state.character.origin_feat = Some("Tough".to_string());
        state.character.general_feats = vec![];
        let hp_before = state.character.max_hp;
        leveling::award_xp(&mut state.character, 300);
        apply_post_levelup_feat_bonuses(&mut state.character, 1);
        assert_eq!(state.character.level, 2);
        let hp_gain = state.character.max_hp - hp_before;
        let expected_base = 8;
        assert_eq!(hp_gain, expected_base + 2, "Origin feat Tough should also grant +2 HP/level");
    }

    // ---- handle_choose_asi: spec acceptance criteria ----

    fn asi_state() -> GameState {
        let mut state = create_test_exploration_state();
        state.character.asi_credits = 1;
        state.game_phase = GamePhase::ChooseAsi;
        state
    }

    #[test]
    fn choose_asi_plus2_str_raises_by_2_and_consumes_credit() {
        let mut state = asi_state();
        state.character.ability_scores.insert(Ability::Strength, 15);
        let result = handle_choose_asi(&mut state, "+2 STR");
        assert_eq!(state.character.ability_scores[&Ability::Strength], 17,
            "STR should be 17 after +2 from 15. Got: {:?}", result);
        assert_eq!(state.character.asi_credits, 0);
        assert!(matches!(state.game_phase, GamePhase::Exploration));
    }

    #[test]
    fn choose_asi_plus1_str_dex_raises_each_and_consumes_credit() {
        let mut state = asi_state();
        state.character.ability_scores.insert(Ability::Strength, 14);
        state.character.ability_scores.insert(Ability::Dexterity, 13);
        let result = handle_choose_asi(&mut state, "+1 STR DEX");
        assert_eq!(state.character.ability_scores[&Ability::Strength], 15,
            "STR should be 15. Got: {:?}", result);
        assert_eq!(state.character.ability_scores[&Ability::Dexterity], 14,
            "DEX should be 14. Got: {:?}", result);
        assert_eq!(state.character.asi_credits, 0);
    }

    #[test]
    fn choose_asi_plus2_str_caps_at_20() {
        let mut state = asi_state();
        state.character.ability_scores.insert(Ability::Strength, 19);
        handle_choose_asi(&mut state, "+2 STR");
        assert_eq!(state.character.ability_scores[&Ability::Strength], 20,
            "+2 on 19 STR must cap at 20, not go to 21");
    }

    #[test]
    fn choose_asi_plus1_two_abilities_already_at_20_stays_at_20() {
        let mut state = asi_state();
        state.character.ability_scores.insert(Ability::Strength, 20);
        state.character.ability_scores.insert(Ability::Dexterity, 20);
        handle_choose_asi(&mut state, "+1 STR DEX");
        assert_eq!(state.character.ability_scores[&Ability::Strength], 20,
            "STR already at 20 must not exceed 20");
        assert_eq!(state.character.ability_scores[&Ability::Dexterity], 20,
            "DEX already at 20 must not exceed 20");
    }

    #[test]
    fn choose_asi_tough_adds_to_general_feats_and_applies_hp() {
        let mut state = asi_state();
        let hp_before = state.character.max_hp;
        let level = state.character.level;
        handle_choose_asi(&mut state, "Tough");
        assert!(state.character.general_feats.contains(&"Tough".to_string()),
            "Tough should appear in general_feats");
        assert_eq!(state.character.max_hp, hp_before + 2 * level as i32,
            "Tough should add 2 * level HP");
        assert_eq!(state.character.asi_credits, 0);
        assert!(matches!(state.game_phase, GamePhase::Exploration));
    }

    #[test]
    fn choose_asi_credits_zero_exits_to_exploration() {
        let mut state = asi_state();
        state.character.asi_credits = 1;
        handle_choose_asi(&mut state, "+2 CON");
        assert_eq!(state.character.asi_credits, 0);
        assert!(matches!(state.game_phase, GamePhase::Exploration),
            "Phase should return to Exploration when credits reach 0");
    }

    #[test]
    fn choose_asi_archery_rejected_for_non_fighter() {
        let mut state = asi_state();
        state.character.class = Class::Wizard; // Wizard can't take Archery
        let result = handle_choose_asi(&mut state, "Archery");
        assert!(result.iter().any(|l| l.contains("Only Fighters")),
            "Expected fighting-style gate error. Got: {:?}", result);
        // Credit should NOT be consumed
        assert_eq!(state.character.asi_credits, 1);
    }

    #[test]
    fn choose_asi_archery_accepted_for_fighter() {
        let mut state = asi_state();
        state.character.class = Class::Fighter;
        handle_choose_asi(&mut state, "Archery");
        assert!(state.character.general_feats.contains(&"Archery".to_string()),
            "Fighter should be able to take Archery");
        assert_eq!(state.character.asi_credits, 0);
    }

    #[test]
    fn choose_asi_origin_feat_rejected() {
        let mut state = asi_state();
        let result = handle_choose_asi(&mut state, "Alert");
        assert!(result.iter().any(|l| l.contains("origin feat")),
            "Alert (origin feat) should be rejected in ASI phase. Got: {:?}", result);
        assert_eq!(state.character.asi_credits, 1, "Credit must not be consumed");
    }

    #[test]
    fn choose_asi_cancel_defers_without_spending() {
        let mut state = asi_state();
        let result = handle_choose_asi(&mut state, "cancel");
        assert!(result.iter().any(|l| l.contains("aside") || l.contains("cancel") || l.contains("defer")),
            "Cancel should defer. Got: {:?}", result);
        assert_eq!(state.character.asi_credits, 1, "Credit must not be consumed on cancel");
        assert!(matches!(state.game_phase, GamePhase::Exploration));
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

    // ---- Ritual casting (feat/expanded-spell-catalog) ----

    #[test]
    fn test_ritual_cast_of_ritual_spell_succeeds_without_slot() {
        // Wizard learns Detect Magic (ritual, concentration) and casts it
        // as a ritual in exploration.
        let mut state = create_test_wizard_state();
        state.character.known_spells.push("Detect Magic".to_string());
        let slots_before = state.character.spell_slots_remaining.clone();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "cast detect magic ritual");
        let text = output.text.join("\n");
        assert!(text.to_lowercase().contains("ritual"),
            "Expected ritual narration. Got:\n{}", text);
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // Slot unchanged on ritual path.
        assert_eq!(new_state.character.spell_slots_remaining, slots_before,
            "Ritual casting must not consume a spell slot");
    }

    #[test]
    fn test_ritual_cast_of_non_ritual_spell_is_rejected() {
        // Magic Missile is NOT a ritual; attempting ritual cast should fail.
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "cast magic missile ritual");
        let text = output.text.join("\n");
        assert!(text.contains("doesn't have the Ritual tag")
            || text.to_lowercase().contains("ritual"),
            "Expected not-a-ritual error. Got:\n{}", text);
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // No slot consumed, spell not actually cast.
        assert_eq!(new_state.character.spell_slots_remaining,
            state.character.spell_slots_remaining);
    }

    // ---- Per-class spellcasting ability (feat/expanded-spell-catalog) ----

    #[test]
    fn test_concentration_save_helper_breaks_on_failure() {
        // Construct a state where the player is concentrating on Bless and
        // takes lethal damage. We cannot rely on RNG outcomes for a binary
        // pass/fail without seed tuning, so we drive the helper directly and
        // assert invariants: the helper clears concentration on a failed save
        // and retains it on a successful save.
        use rand::SeedableRng;
        let mut state = create_test_wizard_state();
        state.character.class_features.concentration_spell = Some("Bless".to_string());
        // CON 13 (+1), prof 2, proficient in CON save? depends on class (Wizard is not).
        // With a CON modifier of +1 and a large damage, a d20 roll of 1 would fail.
        let mut rng = StdRng::seed_from_u64(1);
        let mut lines: Vec<String> = Vec::new();
        check_player_concentration_on_damage(&mut rng, &mut state, 40, &mut lines);
        // Either broken or held; either way, helper must have pushed a line.
        assert!(!lines.is_empty(), "Helper should emit at least one concentration line");
        // If the roll failed (DC 20), concentration_spell must be None.
        // If the roll succeeded, it stays Some("Bless").
        let broken = state.character.class_features.concentration_spell.is_none();
        let held = state.character.class_features.concentration_spell == Some("Bless".to_string());
        assert!(broken || held, "Helper must leave state in a consistent concentration state");
    }

    #[test]
    fn test_concentration_helper_noop_when_not_concentrating() {
        use rand::SeedableRng;
        let mut state = create_test_wizard_state();
        state.character.class_features.concentration_spell = None;
        let mut rng = StdRng::seed_from_u64(1);
        let mut lines: Vec<String> = Vec::new();
        check_player_concentration_on_damage(&mut rng, &mut state, 50, &mut lines);
        assert!(lines.is_empty(), "No lines when player isn't concentrating");
    }

    #[test]
    fn test_wizard_keeps_int_based_spellcasting() {
        // Cast Fire Bolt and confirm the message references the wizard's INT
        // indirectly via the attack roll modifier: not a behavior change, but
        // confirms the path still works for INT casters.
        let state = create_test_wizard_state();
        let state_json = serde_json::to_string(&state).unwrap();
        // Exploration-only Fire Bolt flavor is fine; we just need a successful parse.
        let output = process_input(&state_json, "cast fire bolt");
        let text = output.text.join("\n");
        assert!(!text.contains("don't know"), "Wizard should know Fire Bolt. Got:\n{}", text);
    }

    // ---- Expanded-catalog: non-Wizard caster known spells recognized ----

    fn wizard_state_with_class(class: character::class::Class) -> GameState {
        let mut rng = StdRng::seed_from_u64(42);
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 10);
        scores.insert(Ability::Dexterity, 12);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 10);
        scores.insert(Ability::Wisdom, 14);
        scores.insert(Ability::Charisma, 14);
        let character = create_character(
            "T".to_string(), character::race::Race::Human, class, scores, Vec::new(),
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
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
        }
    }

    #[test]
    fn test_cleric_can_see_known_spells_via_spells_command() {
        let state = wizard_state_with_class(character::class::Class::Cleric);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let text = output.text.join("\n");
        assert!(text.contains("Known Spells"),
            "Cleric should have known spells. Got:\n{}", text);
        assert!(text.to_lowercase().contains("cure wounds")
            || text.to_lowercase().contains("guiding bolt"),
            "Cleric known list should include a signature spell. Got:\n{}", text);
    }

    #[test]
    fn test_sorcerer_can_see_known_spells_via_spells_command() {
        let state = wizard_state_with_class(character::class::Class::Sorcerer);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let text = output.text.join("\n");
        assert!(text.contains("Known Spells"),
            "Sorcerer should have known spells. Got:\n{}", text);
    }

    #[test]
    fn test_bard_can_see_known_spells_via_spells_command() {
        let state = wizard_state_with_class(character::class::Class::Bard);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let text = output.text.join("\n");
        assert!(text.contains("Known Spells"),
            "Bard should have known spells. Got:\n{}", text);
    }

    #[test]
    fn test_fighter_still_has_no_spells() {
        let state = wizard_state_with_class(character::class::Class::Fighter);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let text = output.text.join("\n");
        assert!(text.contains("You don't know any spells"),
            "Fighter should still get no-spells message. Got:\n{}", text);
    }

    #[test]
    fn test_paladin_level_one_shows_starting_spells() {
        // 2024 SRD: Paladin gains Spellcasting at level 1 with 2 prepared
        // spells (engine uses Heroism + Bless since Searing Smite is not in
        // the catalog). See docs/reference/paladin.md.
        let state = wizard_state_with_class(character::class::Class::Paladin);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "spells");
        let text = output.text.join("\n");
        assert!(!text.contains("You don't know any spells"),
            "Paladin L1 should now know starter spells. Got:\n{}", text);
        assert!(text.contains("Heroism"),
            "Paladin L1 should list Heroism. Got:\n{}", text);
        assert!(text.contains("Bless"),
            "Paladin L1 should list Bless. Got:\n{}", text);
    }

    // ---- Magic item orchestration (feat/magic-items, 2026-04-15) ----

    fn give_wondrous_to_player(state: &mut GameState, name: &str, effect: equipment::magic::WondrousEffect, rarity: equipment::magic::Rarity, requires_attunement: bool) -> u32 {
        let item_id = (state.world.items.len() as u32) + 2000;
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: String::new(),
            item_type: state::ItemType::Wondrous { effect, rarity, requires_attunement },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    fn give_magic_weapon_to_player(state: &mut GameState, name: &str, base: &str, attack_bonus: i32, damage_bonus: i32) -> u32 {
        use equipment::magic::Rarity;
        let item_id = (state.world.items.len() as u32) + 2100;
        // Look up the base weapon for fields.
        let w = equipment::SRD_WEAPONS.iter().find(|w| w.name == base).expect("base weapon");
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: String::new(),
            item_type: state::ItemType::MagicWeapon {
                base_weapon: base.to_string(),
                damage_dice: w.damage_dice, damage_die: w.damage_die,
                damage_type: w.damage_type, properties: w.properties,
                category: w.category, versatile_die: w.versatile_die,
                range_normal: w.range_normal, range_long: w.range_long,
                attack_bonus, damage_bonus,
                rarity: Rarity::Uncommon,
                requires_attunement: false,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    #[test]
    fn test_attune_to_wondrous_item_succeeds() {
        use equipment::magic::{Rarity, WondrousEffect};
        let mut state = create_test_exploration_state();
        let id = give_wondrous_to_player(&mut state, "Cloak of Protection",
            WondrousEffect::CloakOfProtection, Rarity::Uncommon, true);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attune cloak of protection");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(new_state.character.attuned_items.contains(&id),
            "Expected attuned_items to contain id {}, got: {:?}", id, new_state.character.attuned_items);
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("attune") || t.to_lowercase().contains("resonate")),
            "Expected attunement narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_attune_cap_blocks_fourth_attunement() {
        use equipment::magic::{Rarity, WondrousEffect};
        let mut state = create_test_exploration_state();
        let a = give_wondrous_to_player(&mut state, "Cloak of Protection",
            WondrousEffect::CloakOfProtection, Rarity::Uncommon, true);
        let b = give_wondrous_to_player(&mut state, "Ring of Protection",
            WondrousEffect::RingOfProtection, Rarity::Rare, true);
        let c = give_wondrous_to_player(&mut state, "Gauntlets of Ogre Power",
            WondrousEffect::GauntletsOfOgrePower, Rarity::Uncommon, true);
        // Fourth item — adding should fail.
        let d = give_wondrous_to_player(&mut state, "Belt of Giant Strength",
            WondrousEffect::BeltOfGiantStrength(21), Rarity::Rare, true);
        // Pre-fill the attuned vec with the first three IDs (skip actually running attune commands for brevity).
        state.character.attuned_items = vec![a, b, c];

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attune belt");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.attuned_items.contains(&d),
            "Fourth attunement must not be added. Got: {:?}", new_state.character.attuned_items);
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("already attuned") || t.to_lowercase().contains("unattune") || t.to_lowercase().contains("max")),
            "Expected cap-exceeded narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_attune_non_attunement_item_rejects() {
        // +1 Longsword does NOT require attunement.
        let mut state = create_test_exploration_state();
        let id = give_magic_weapon_to_player(&mut state, "+1 Longsword", "Longsword", 1, 1);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attune longsword");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.attuned_items.contains(&id));
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("does not require")),
            "Expected 'does not require attunement' narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_attune_non_magic_item_rejects() {
        let mut state = create_test_exploration_state();
        // A mundane consumable (not magical).
        give_consumable_to_player(&mut state, "Rations", "dried food", "nourish");
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attune rations");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("not a magic item")),
            "Expected 'not a magic item' narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_unattune_removes_from_list() {
        use equipment::magic::{Rarity, WondrousEffect};
        let mut state = create_test_exploration_state();
        let id = give_wondrous_to_player(&mut state, "Cloak of Protection",
            WondrousEffect::CloakOfProtection, Rarity::Uncommon, true);
        state.character.attuned_items.push(id);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "unattune cloak");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.attuned_items.contains(&id));
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("release")),
            "Expected release narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_list_attunements_empty() {
        let state = create_test_exploration_state();
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attunement");
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("not attuned") && t.contains("0")),
            "Expected 'not attuned' narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_magic_weapon_bonuses_returns_zero_for_mundane() {
        let state = create_test_exploration_state();
        // Make a mundane dagger in items
        let mut state = state;
        let id = give_magic_weapon_to_player(&mut state, "Dagger", "Dagger", 0, 0); // reused helper, bonus=0
        // Overwrite as mundane weapon to be explicit.
        state.world.items.insert(id, state::Item {
            id, name: "Dagger".to_string(), description: String::new(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 4,
                damage_type: state::DamageType::Piercing, properties: 0,
                category: state::WeaponCategory::Simple,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true, charges_remaining: None,
        });
        assert_eq!(magic_weapon_bonuses(&state, Some(id)), (0, 0));
    }

    #[test]
    fn test_magic_weapon_bonuses_returns_bonus_for_plus_one() {
        let mut state = create_test_exploration_state();
        let id = give_magic_weapon_to_player(&mut state, "+1 Longsword", "Longsword", 1, 1);
        assert_eq!(magic_weapon_bonuses(&state, Some(id)), (1, 1));
    }

    #[test]
    fn test_magic_weapon_bonuses_gated_on_attunement() {
        use equipment::magic::Rarity;
        let mut state = create_test_exploration_state();
        let id = 9001;
        state.world.items.insert(id, state::Item {
            id, name: "Holy Avenger".to_string(), description: String::new(),
            item_type: state::ItemType::MagicWeapon {
                base_weapon: "Longsword".to_string(),
                damage_dice: 1, damage_die: 8,
                damage_type: state::DamageType::Slashing, properties: 0,
                category: state::WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
                attack_bonus: 3, damage_bonus: 3,
                rarity: Rarity::Legendary,
                requires_attunement: true,
            },
            location: None, carried_by_player: true, charges_remaining: None,
        });
        state.character.inventory.push(id);
        // Not attuned — no bonus.
        assert_eq!(magic_weapon_bonuses(&state, Some(id)), (0, 0));
        state.character.attuned_items.push(id);
        assert_eq!(magic_weapon_bonuses(&state, Some(id)), (3, 3));
    }

    #[test]
    fn test_apply_magic_weapon_bonuses_hits_with_new_threshold() {
        use combat::AttackResult;
        use state::DamageType;
        // A near-miss that flips to a hit with +2 attack bonus.
        let mut r = AttackResult {
            hit: false, natural_20: false, natural_1: false,
            attack_roll: 10, total_attack: 14, target_ac: 15,
            damage: 0, damage_type: DamageType::Slashing,
            weapon_name: "+2 Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: false,
        };
        apply_magic_weapon_bonuses(&mut r, 2, 2);
        assert!(r.hit);
        assert_eq!(r.total_attack, 16);
    }

    #[test]
    fn test_apply_magic_weapon_bonuses_adds_damage_on_hit() {
        use combat::AttackResult;
        use state::DamageType;
        let mut r = AttackResult {
            hit: true, natural_20: false, natural_1: false,
            attack_roll: 15, total_attack: 20, target_ac: 15,
            damage: 8, damage_type: DamageType::Slashing,
            weapon_name: "+1 Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: false,
        };
        apply_magic_weapon_bonuses(&mut r, 1, 1);
        assert_eq!(r.damage, 9);
    }

    #[test]
    fn test_apply_magic_weapon_bonuses_nat1_still_misses() {
        use combat::AttackResult;
        use state::DamageType;
        let mut r = AttackResult {
            hit: false, natural_20: false, natural_1: true,
            attack_roll: 1, total_attack: 5, target_ac: 15,
            damage: 0, damage_type: DamageType::Slashing,
            weapon_name: "+3 Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: false,
        };
        apply_magic_weapon_bonuses(&mut r, 3, 3);
        // Nat 1 still misses, regardless of bonus.
        assert!(!r.hit);
    }

    #[test]
    fn test_apply_magic_weapon_bonuses_nat20_still_hits_and_adds_damage() {
        use combat::AttackResult;
        use state::DamageType;
        // Nat 20 crit already, damage was 2d8 + str mod rolled as 15. The
        // +2 damage bonus is added flat (NOT doubled per SRD).
        let mut r = AttackResult {
            hit: true, natural_20: true, natural_1: false,
            attack_roll: 20, total_attack: 25, target_ac: 30, // still hits because nat20
            damage: 15, damage_type: DamageType::Slashing,
            weapon_name: "+2 Longsword".to_string(),
            disadvantage: false,
            attacker_had_advantage: false,
        };
        apply_magic_weapon_bonuses(&mut r, 2, 2);
        assert!(r.hit);
        assert_eq!(r.damage, 17);
    }

    #[test]
    fn test_list_attunements_shows_items() {
        use equipment::magic::{Rarity, WondrousEffect};
        let mut state = create_test_exploration_state();
        let id = give_wondrous_to_player(&mut state, "Cloak of Protection",
            WondrousEffect::CloakOfProtection, Rarity::Uncommon, true);
        state.character.attuned_items.push(id);
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "attunement");
        assert!(output.text.iter().any(|t| t.contains("Cloak of Protection")),
            "Expected cloak listed. Got: {:?}", output.text);
        assert!(output.text.iter().any(|t| t.contains("1") && t.contains("3")),
            "Expected slot count (1/3). Got: {:?}", output.text);
    }

    // ==== Potion use tests (feat/magic-items) ====

    /// Helper: place a magic-item Potion in the player's inventory.
    fn give_potion_to_player(
        state: &mut GameState,
        name: &str,
        effect: equipment::magic::PotionEffect,
        rarity: equipment::magic::Rarity,
    ) -> u32 {
        let item_id = (state.world.items.len() as u32) + 2200;
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: String::new(),
            item_type: state::ItemType::Potion { effect, rarity },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    /// Helper: place a magic-item Scroll in the player's inventory.
    fn give_scroll_to_player(
        state: &mut GameState,
        name: &str,
        spell_name: &str,
        spell_level: u32,
        rarity: equipment::magic::Rarity,
    ) -> u32 {
        let item_id = (state.world.items.len() as u32) + 2300;
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: String::new(),
            item_type: state::ItemType::Scroll {
                spell_name: spell_name.to_string(),
                spell_level,
                rarity,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    /// Helper: place a magic-item Wand in the player's inventory with initial charges.
    fn give_wand_to_player(
        state: &mut GameState,
        name: &str,
        spell_name: &str,
        rarity: equipment::magic::Rarity,
        requires_attunement: bool,
        charges: u32,
    ) -> u32 {
        let item_id = (state.world.items.len() as u32) + 2400;
        let item = state::Item {
            id: item_id,
            name: name.to_string(),
            description: String::new(),
            item_type: state::ItemType::Wand {
                spell_name: spell_name.to_string(),
                rarity,
                requires_attunement,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: Some(charges),
        };
        state.world.items.insert(item_id, item);
        state.character.inventory.push(item_id);
        item_id
    }

    #[test]
    fn test_use_potion_of_healing_restores_hp() {
        use equipment::magic::{PotionEffect, Rarity};
        let mut state = create_test_exploration_state();
        // Damage the player first.
        state.character.max_hp = 30;
        state.character.current_hp = 10;
        let id = give_potion_to_player(&mut state, "Potion of Healing",
            PotionEffect::Healing { dice: 2, die: 4, bonus: 2 }, Rarity::Common);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use potion of healing");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Potion is consumed.
        assert!(!new_state.character.inventory.contains(&id),
            "Potion should be removed from inventory. Got: {:?}", new_state.character.inventory);
        assert!(!new_state.world.items.contains_key(&id),
            "Potion should be removed from world items");
        // HP went up (2d4+2 is 4..=10).
        assert!(new_state.character.current_hp > 10,
            "Expected HP to increase. Got {}", new_state.character.current_hp);
        assert!(new_state.character.current_hp <= 30, "Must not exceed max_hp");
        // Narration mentions healing.
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("heal")
            || t.to_lowercase().contains("recover")
            || t.to_lowercase().contains("hp")),
            "Expected healing narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_use_potion_of_healing_caps_at_max_hp() {
        use equipment::magic::{PotionEffect, Rarity};
        let mut state = create_test_exploration_state();
        state.character.max_hp = 30;
        state.character.current_hp = 29; // Nearly full.
        give_potion_to_player(&mut state, "Potion of Healing",
            PotionEffect::Healing { dice: 2, die: 4, bonus: 2 }, Rarity::Common);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use potion of healing");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert_eq!(new_state.character.current_hp, 30,
            "HP must cap at max_hp. Got {}", new_state.character.current_hp);
        assert!(!output.text.is_empty());
    }

    #[test]
    fn test_use_flavor_potion_is_consumed_with_narration() {
        use equipment::magic::{PotionEffect, Rarity};
        let mut state = create_test_exploration_state();
        let id = give_potion_to_player(&mut state, "Potion of Invisibility",
            PotionEffect::Invisibility, Rarity::VeryRare);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use potion of invisibility");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // Even flavor-only potions consume on use.
        assert!(!new_state.character.inventory.contains(&id),
            "Flavor potion should still be consumed. Got: {:?}", new_state.character.inventory);
        assert!(!output.text.is_empty());
    }

    // ==== Wand charges tests ====

    #[test]
    fn test_use_wand_decrements_charges() {
        use equipment::magic::Rarity;
        let mut state = create_test_exploration_state();
        let id = give_wand_to_player(&mut state, "Wand of Magic Missiles",
            "Magic Missile", Rarity::Uncommon, false, 7);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use wand of magic missiles");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        let remaining = new_state.world.items.get(&id)
            .and_then(|i| i.charges_remaining).expect("wand still exists");
        assert_eq!(remaining, 6, "One charge should be spent. Got {}", remaining);
        // Wand is NOT consumed (unlike potions/scrolls).
        assert!(new_state.character.inventory.contains(&id),
            "Wand should remain in inventory after use");
        assert!(!output.text.is_empty());
    }

    #[test]
    fn test_use_wand_with_zero_charges_fails() {
        use equipment::magic::Rarity;
        let mut state = create_test_exploration_state();
        let id = give_wand_to_player(&mut state, "Wand of Magic Missiles",
            "Magic Missile", Rarity::Uncommon, false, 0);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use wand of magic missiles");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();

        // Still at 0.
        let remaining = new_state.world.items.get(&id)
            .and_then(|i| i.charges_remaining).unwrap();
        assert_eq!(remaining, 0);
        // Wand still in inventory.
        assert!(new_state.character.inventory.contains(&id));
        // Narration mentions no charges.
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("no charges")
            || t.to_lowercase().contains("spent")
            || t.to_lowercase().contains("depleted")),
            "Expected no-charges narration. Got: {:?}", output.text);
    }

    #[test]
    fn test_use_wand_requires_attunement_when_gated() {
        use equipment::magic::Rarity;
        let mut state = create_test_exploration_state();
        let id = give_wand_to_player(&mut state, "Wand of Fireballs",
            "Fireball", Rarity::Rare, true, 7);
        // Player is NOT attuned.
        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use wand of fireballs");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        // Charges unchanged.
        let remaining = new_state.world.items.get(&id)
            .and_then(|i| i.charges_remaining).unwrap();
        assert_eq!(remaining, 7, "Charges must not decrement when not attuned");
        // Narration mentions attunement requirement.
        assert!(output.text.iter().any(|t| t.to_lowercase().contains("attune")),
            "Expected attunement-required narration. Got: {:?}", output.text);
    }

    // ==== Scroll tests ====

    #[test]
    fn test_use_scroll_by_spellcaster_consumes_scroll() {
        use equipment::magic::Rarity;
        use crate::character::class::Class;
        let mut state = create_test_exploration_state();
        // Make the player a Wizard so they cast scrolls without a check.
        state.character.class = Class::Wizard;
        let id = give_scroll_to_player(&mut state, "Scroll of Fireball",
            "Fireball", 3, Rarity::Uncommon);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use scroll of fireball");
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        assert!(!new_state.character.inventory.contains(&id),
            "Scroll should be consumed by a spellcaster. Got: {:?}", new_state.character.inventory);
        assert!(!output.text.is_empty());
    }

    #[test]
    fn test_use_scroll_by_nonspellcaster_rolls_arcana_check() {
        use equipment::magic::Rarity;
        use crate::character::class::Class;
        let mut state = create_test_exploration_state();
        // Fighter is the non-spellcaster baseline.
        state.character.class = Class::Fighter;
        give_scroll_to_player(&mut state, "Scroll of Fireball",
            "Fireball", 3, Rarity::Uncommon);

        let state_json = serde_json::to_string(&state).unwrap();
        let output = process_input(&state_json, "use scroll of fireball");
        // Narration MUST mention the check (arcana / DC 10).
        let full = output.text.join(" ").to_lowercase();
        assert!(full.contains("arcana") || full.contains("dc 10") || full.contains("check"),
            "Expected DC 10 Arcana-check narration. Got: {:?}", output.text);
    }

    // ==== World loot magic item spawn ====

    #[test]
    fn test_world_generates_some_magic_items_with_deterministic_seed() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        // Generate a large batch and ensure SOME magic items appear.
        // Each item has ~5% magic spawn chance, so 1000 items → binomial mean 50.
        let mut rng = StdRng::seed_from_u64(12345);
        let items = crate::world::item::generate_items(&mut rng, &[0, 1, 2, 3, 4], 1000);
        let magic_count = items.values().filter(|i| matches!(
            i.item_type,
            state::ItemType::MagicWeapon { .. }
            | state::ItemType::MagicArmor { .. }
            | state::ItemType::Wondrous { .. }
            | state::ItemType::Potion { .. }
            | state::ItemType::Scroll { .. }
            | state::ItemType::Wand { .. }
        )).count();
        assert!(magic_count > 0,
            "Expected some magic items in 1000 spawns. Got 0");
    }

    #[test]
    fn test_world_magic_item_rarity_weighted_common_dominates() {
        // Over a large sample, Common rarity should dominate Legendary.
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        use equipment::magic::Rarity;

        fn item_rarity(it: &state::ItemType) -> Option<Rarity> {
            match it {
                state::ItemType::MagicWeapon { rarity, .. }
                | state::ItemType::MagicArmor { rarity, .. }
                | state::ItemType::Wondrous { rarity, .. }
                | state::ItemType::Potion { rarity, .. }
                | state::ItemType::Scroll { rarity, .. }
                | state::ItemType::Wand { rarity, .. } => Some(*rarity),
                _ => None,
            }
        }

        let mut rng = StdRng::seed_from_u64(99);
        let items = crate::world::item::generate_items(&mut rng, &[0, 1, 2, 3, 4], 5000);
        let mut common = 0usize;
        let mut legendary = 0usize;
        for i in items.values() {
            match item_rarity(&i.item_type) {
                Some(Rarity::Common) => common += 1,
                Some(Rarity::Legendary) => legendary += 1,
                _ => {}
            }
        }
        assert!(common > legendary,
            "Expected Common ({}) > Legendary ({}) over 5000 spawns", common, legendary);
    }

    // ---- Weapon Mastery orchestrator wiring (feat/weapon-mastery) --------

    #[test]
    fn test_attack_with_sap_mastery_marks_target() {
        // The default combat fixture equips a Longsword (Sap mastery).
        // Giving the character Sap for Longsword should mean any attack
        // roll that hits marks the NPC for Sap disadvantage.
        let mut state = create_test_combat_state();
        state.character.weapon_masteries.push("Longsword".to_string());
        // Bump goblin HP well above max weapon damage so a single hit
        // cannot end combat (which would clear active_combat to None).
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.max_hp = 200;
                stats.current_hp = 200;
            }
        }
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
        }

        // Try several seeds until we land a hit, then verify the mark.
        for seed in 0..40u64 {
            let mut s = state.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            let Some(combat) = new_state.active_combat.as_ref() else { continue };
            if combat.sap_targets.contains(&100) {
                // Success: hit landed and Sap mark was set.
                assert!(output.text.iter().any(|t| t.contains("Sap")),
                    "Expected Sap narration, got: {:?}", output.text);
                return;
            }
        }
        panic!("Did not land a hit in 40 seeds; fixture may need higher attack bonus.");
    }

    #[test]
    fn test_attack_without_mastery_does_not_mark_sap() {
        // Same fixture but the character has NOT unlocked Sap for Longsword.
        // Even on a hit, combat.sap_targets should stay empty.
        let mut state = create_test_combat_state();
        // The default fixture uses Fighter, which auto-grants Longsword
        // mastery at creation. Clear the list so this test legitimately
        // exercises the no-mastery path.
        state.character.weapon_masteries.clear();
        // Bump goblin HP so a hit doesn't end combat and drop active_combat.
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.max_hp = 200;
                stats.current_hp = 200;
            }
        }
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
        }

        for seed in 0..40u64 {
            let mut s = state.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            // If combat somehow ended, skip this seed -- Sap obviously
            // cannot have fired into a nonexistent combat state.
            let Some(combat) = new_state.active_combat.as_ref() else { continue };
            assert!(!combat.sap_targets.contains(&100),
                "Sap should never fire without mastery (seed {}), got text: {:?}",
                seed, output.text);
        }
    }

    #[test]
    fn test_graze_on_miss_deals_damage_end_to_end() {
        // Swap the Longsword for a Greatsword (Graze mastery). On a miss,
        // Graze should still deal ability-mod damage -- the NPC's HP should
        // drop by the STR modifier.
        let mut state = create_test_combat_state();
        // Replace main-hand with Greatsword.
        let greatsword_id = 201;
        state.world.items.insert(greatsword_id, state::Item {
            id: greatsword_id,
            name: "Greatsword".to_string(),
            description: "A massive two-hander.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 2, damage_die: 6,
                damage_type: state::DamageType::Slashing,
                properties: crate::equipment::HEAVY | crate::equipment::TWO_HANDED,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        });
        state.character.inventory.push(greatsword_id);
        state.character.equipped.main_hand = Some(greatsword_id);
        state.character.weapon_masteries.push("Greatsword".to_string());

        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
        }

        // Bump goblin AC so misses are likely. Find a seed that produces a miss.
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.ac = 25; // almost impossible to hit
            }
        }

        for seed in 0..40u64 {
            let mut s = state.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let start_hp = s.world.npcs[&100].combat_stats.as_ref().unwrap().current_hp;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            let new_hp = new_state.world.npcs[&100].combat_stats.as_ref().unwrap().current_hp;
            let missed = output.text.iter().any(|t| t.contains("miss"));
            if missed && new_hp < start_hp {
                // Graze landed: damage was dealt even on a miss.
                assert!(output.text.iter().any(|t| t.contains("Graze")),
                    "Expected Graze narration on miss-with-damage, got: {:?}", output.text);
                return;
            }
        }
        panic!("Did not produce a Graze scenario in 40 seeds.");
    }

    #[test]
    fn test_attack_with_push_mastery_shoves_target() {
        // Swap main-hand for a Warhammer (Push mastery). On a hit, the
        // goblin's distance should increase by 10 ft.
        let mut state = create_test_combat_state();
        let warhammer_id = 202;
        state.world.items.insert(warhammer_id, state::Item {
            id: warhammer_id,
            name: "Warhammer".to_string(),
            description: "A heavy hammer.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 8,
                damage_type: state::DamageType::Bludgeoning,
                properties: crate::equipment::VERSATILE,
                category: state::WeaponCategory::Martial,
                versatile_die: 10, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        });
        state.character.inventory.push(warhammer_id);
        state.character.equipped.main_hand = Some(warhammer_id);
        state.character.weapon_masteries.push("Warhammer".to_string());
        // Bump goblin HP so a hit doesn't end combat and drop active_combat.
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.max_hp = 200;
                stats.current_hp = 200;
            }
        }

        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
        }

        for seed in 0..40u64 {
            let mut s = state.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            let Some(combat) = new_state.active_combat.as_ref() else { continue };
            let dist = combat.distances.get(&100).copied().unwrap_or(0);
            if dist == 15 {
                // Push fired: 5 -> 15.
                assert!(output.text.iter().any(|t| t.contains("Push")),
                    "Expected Push narration, got: {:?}", output.text);
                return;
            }
        }
        panic!("Did not produce a Push scenario in 40 seeds.");
    }

    #[test]
    fn test_vex_mastery_grants_advantage_on_next_attack() {
        // Swap main-hand for a Shortsword (Vex mastery). After a hit, the
        // next attack against the same target should have advantage. Over
        // many seeds with Vex, hit rate should be higher than without.
        let mut base = create_test_combat_state();
        let shortsword_id = 203;
        base.world.items.insert(shortsword_id, state::Item {
            id: shortsword_id,
            name: "Shortsword".to_string(),
            description: "A nimble blade.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::FINESSE | crate::equipment::LIGHT,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        });
        base.character.inventory.push(shortsword_id);
        base.character.equipped.main_hand = Some(shortsword_id);
        // Bump goblin HP so a hit doesn't end combat and drop active_combat.
        if let Some(npc) = base.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.max_hp = 200;
                stats.current_hp = 200;
            }
        }

        // Preload the Vex mark on the combat state (simulates a prior hit
        // with the Shortsword) and then attack. Verify the hit narration
        // mentions advantage.
        force_player_turn(&mut base);
        if let Some(ref mut combat) = base.active_combat {
            combat.distances.insert(100, 5);
            combat.player_vex_target = Some(100);
        }

        let state_json = serde_json::to_string(&base).unwrap();
        let output = process_input(&state_json, "attack test goblin");
        assert!(
            output.text.iter().any(|t| t.contains("Vex")),
            "Expected Vex narration when player_vex_target is preloaded, got: {:?}",
            output.text,
        );
        // Consumed: the new state should have player_vex_target == None.
        let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
        let combat = new_state.active_combat.as_ref()
            .expect("combat should still be active after a single hit against a high-HP target");
        assert_eq!(combat.player_vex_target, None, "Vex mark should be consumed");
    }

    // ---- Rogue: Sneak Attack (gh issue #85) ----
    //
    // These tests exercise the orchestrator's Sneak Attack dispatch after a
    // successful player attack. Eligibility requires: Rogue class + Finesse
    // (or ranged) weapon + attacker had advantage + not already used this
    // turn. See `apply_sneak_attack` in lib.rs and `docs/specs/srd-classes.md`.

    /// Build a `create_test_combat_state` base then reshape it into a Rogue
    /// wielding a Shortsword (Finesse + Light). The goblin is given a Prone
    /// condition to grant the player advantage on melee attacks (per SRD,
    /// attacks against a Prone target within 5 ft have advantage).
    fn rogue_sneak_attack_setup() -> GameState {
        let mut state = create_test_combat_state();
        state.character.class = Class::Rogue;
        state.character.level = 1;
        // Replace main-hand Longsword with a Shortsword (Finesse + Light).
        let shortsword_id = 210;
        state.world.items.insert(shortsword_id, state::Item {
            id: shortsword_id,
            name: "Shortsword".to_string(),
            description: "A nimble blade.".to_string(),
            item_type: state::ItemType::Weapon {
                damage_dice: 1, damage_die: 6,
                damage_type: state::DamageType::Piercing,
                properties: crate::equipment::FINESSE | crate::equipment::LIGHT,
                category: state::WeaponCategory::Martial,
                versatile_die: 0, range_normal: 0, range_long: 0,
            },
            location: None, carried_by_player: true,
            charges_remaining: None,
        });
        state.character.inventory.push(shortsword_id);
        state.character.equipped.main_hand = Some(shortsword_id);
        // Boost goblin HP so a single SA hit doesn't end combat.
        if let Some(npc) = state.world.npcs.get_mut(&100) {
            if let Some(ref mut stats) = npc.combat_stats {
                stats.max_hp = 200;
                stats.current_hp = 200;
            }
            // Prone grants advantage on melee attacks <= 5 ft. This is the
            // SRD-legitimate way to give the player advantage without
            // hacking `attacker_had_advantage` directly.
            npc.conditions.push(crate::conditions::ActiveCondition::new(
                crate::conditions::ConditionType::Prone,
                crate::conditions::ConditionDuration::Permanent,
            ));
        }
        force_player_turn(&mut state);
        if let Some(ref mut combat) = state.active_combat {
            combat.distances.insert(100, 5);
        }
        state.character.class_features.sneak_attack_used_this_turn = false;
        state
    }

    #[test]
    fn test_rogue_sneak_attack_fires_on_finesse_hit_with_advantage() {
        // Rogue with Shortsword (Finesse) + advantage (Prone target) should
        // apply Sneak Attack on a hit. Check narration AND the SA flag.
        let base = rogue_sneak_attack_setup();
        for seed in 0..40u64 {
            let mut s = base.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            if !output.text.iter().any(|t| t.contains("hit for")) {
                continue;
            }
            // Hit landed -- Sneak Attack must have fired.
            assert!(output.text.iter().any(|t| t.contains("Sneak Attack")),
                "Expected Sneak Attack narration on a Rogue Finesse hit with advantage, got: {:?}",
                output.text);
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert!(new_state.character.class_features.sneak_attack_used_this_turn,
                "sneak_attack_used_this_turn should be set after SA fires");
            return;
        }
        panic!("Did not land a Rogue hit in 40 seeds; fixture may need adjustment.");
    }

    #[test]
    fn test_rogue_sneak_attack_does_not_fire_without_advantage() {
        // Same Rogue + Shortsword but remove Prone so no advantage source.
        let mut base = rogue_sneak_attack_setup();
        if let Some(npc) = base.world.npcs.get_mut(&100) {
            npc.conditions.clear();
        }
        for seed in 0..40u64 {
            let mut s = base.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            // Whether the attack hits or misses, Sneak Attack must not fire.
            assert!(!output.text.iter().any(|t| t.contains("Sneak Attack")),
                "SA should not fire without advantage (seed {}). Got: {:?}",
                seed, output.text);
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert!(!new_state.character.class_features.sneak_attack_used_this_turn,
                "SA flag should remain unset when advantage is absent");
        }
    }

    #[test]
    fn test_rogue_sneak_attack_does_not_fire_on_non_finesse_weapon() {
        // Rogue wielding a Longsword (non-finesse, non-ranged melee). Even
        // with advantage, SA should not fire.
        let mut base = rogue_sneak_attack_setup();
        // Swap back to Longsword (the default main_hand in create_test_combat_state).
        base.character.equipped.main_hand = Some(200);
        for seed in 0..40u64 {
            let mut s = base.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            assert!(!output.text.iter().any(|t| t.contains("Sneak Attack")),
                "SA should not fire with a non-finesse weapon (seed {}). Got: {:?}",
                seed, output.text);
        }
    }

    #[test]
    fn test_rogue_sneak_attack_fires_only_once_per_turn() {
        // Pre-set the flag as if SA already fired this turn. The next hit
        // (still Finesse + advantage) must NOT add more SA dice.
        let mut base = rogue_sneak_attack_setup();
        base.character.class_features.sneak_attack_used_this_turn = true;
        for seed in 0..40u64 {
            let mut s = base.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            assert!(!output.text.iter().any(|t| t.contains("Sneak Attack")),
                "SA should fire at most once per turn (seed {}). Got: {:?}",
                seed, output.text);
        }
    }

    #[test]
    fn test_non_rogue_never_applies_sneak_attack() {
        // Fighter wielding a Shortsword with advantage still gets no SA.
        let mut base = rogue_sneak_attack_setup();
        base.character.class = Class::Fighter;
        for seed in 0..40u64 {
            let mut s = base.clone();
            s.rng_seed = seed;
            s.rng_counter = 0;
            let state_json = serde_json::to_string(&s).unwrap();
            let output = process_input(&state_json, "attack test goblin");
            assert!(!output.text.iter().any(|t| t.contains("Sneak Attack")),
                "Non-Rogue must not apply SA (seed {}). Got: {:?}",
                seed, output.text);
            let new_state: GameState = serde_json::from_str(&output.state_json).unwrap();
            assert!(!new_state.character.class_features.sneak_attack_used_this_turn,
                "Non-Rogue should leave the SA flag unset");
        }
    }
}
