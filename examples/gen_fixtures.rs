/// Helper binary: generate fixture JSON files used by dev-mode tests.
/// Run with: cargo run -p jurnalis-engine --example gen_fixtures
///
/// Outputs:
///   fixtures/combat-fighter-vs-goblin.json
///   fixtures/post-short-rest.json
use jurnalis_engine::{new_game, process_input};
use jurnalis_engine::state::GameState;

fn advance_char_creation(state_json: &str) -> String {
    // Human Fighter Soldier character creation
    // Race list: 1=Human (no subrace -> skip to class)
    // Class list: 1=Barbarian,2=Bard,3=Cleric,4=Druid,5=Fighter,...
    // Background: 1=Acolyte,... 15=Soldier
    // Origin feat: 1=Alert
    // Ability pattern: 1=+2/+1
    // Ability method: 1=Standard Array
    // Assign: STR DEX CON INT WIS CHA
    // Skills: Fighter gets 2 picks from its list
    // Alignment: 1=LG
    // Name: Aria
    let steps = [
        "1",            // ChooseRace -> Human (no subrace -> ChooseClass)
        "5",            // ChooseClass -> Fighter
        "15",           // ChooseBackground -> Soldier
        "1",            // ChooseOriginFeat -> Alert
        "1",            // ChooseBackgroundAbilityPattern -> +2/+1
        "1",            // ChooseAbilityMethod -> Standard Array
        "15 14 13 12 10 8", // AssignAbilities -> STR=15 DEX=14 CON=13 INT=12 WIS=10 CHA=8
        "1 2",          // ChooseSkills -> pick 2 (Athletics, Acrobatics for Fighter)
        "1",            // ChooseAlignment -> Lawful Good
        "Aria",         // ChooseName
    ];
    let mut s = state_json.to_string();
    for (i, step) in steps.iter().enumerate() {
        let st: GameState = serde_json::from_str(&s).unwrap();
        let out = process_input(&s, step);
        if out.state_json.is_empty() {
            panic!("Step {} ('{}') produced empty state. Phase: {:?}", i, step, st.game_phase);
        }
        s = out.state_json;
    }
    s
}

fn main() {
    std::fs::create_dir_all("fixtures").unwrap();

    // Complete character creation to reach Exploration phase
    let initial = new_game(42, false);
    let exploration_state = advance_char_creation(&initial.state_json);

    let phase: GameState = serde_json::from_str(&exploration_state).unwrap();
    eprintln!("After creation: phase={:?}, name={}", phase.game_phase, phase.character.name);

    // --- Fixture 2: post-short-rest ---
    let out = process_input(&exploration_state, "short rest");
    let post_rest_state = if !out.state_json.is_empty() {
        out.state_json
    } else {
        exploration_state.clone()
    };
    std::fs::write("fixtures/post-short-rest.json", &post_rest_state).unwrap();
    println!("Written fixtures/post-short-rest.json");

    // --- Fixture 1: combat-fighter-vs-goblin ---
    // Walk locations to find a hostile NPC to attack
    let mut combat_state = exploration_state.clone();
    let mut current = exploration_state.clone();
    let directions = ["north", "south", "east", "west", "up", "down"];

    'outer: for _ in 0..10 {
        let st: GameState = serde_json::from_str(&current).unwrap();
        if let Some(loc) = st.world.locations.get(&st.current_location) {
            for &npc_id in &loc.npcs {
                if let Some(npc) = st.world.npcs.get(&npc_id) {
                    let out = process_input(&current, &format!("attack {}", npc.name));
                    let after: GameState = serde_json::from_str(&out.state_json).unwrap();
                    if after.active_combat.is_some() {
                        combat_state = out.state_json;
                        eprintln!("Started combat with: {}", npc.name);
                        break 'outer;
                    }
                }
            }
        }
        // Move to a new location
        for dir in &directions {
            let out = process_input(&current, dir);
            let new: GameState = serde_json::from_str(&out.state_json).unwrap();
            if new.current_location != st.current_location {
                current = out.state_json;
                break;
            }
        }
    }

    std::fs::write("fixtures/combat-fighter-vs-goblin.json", &combat_state).unwrap();
    println!("Written fixtures/combat-fighter-vs-goblin.json");
}
