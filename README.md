<div align="center">
  <img src="assets/wordmark.png" alt="jurnalis-engine" width="444" />

  [![Crates.io](https://img.shields.io/crates/v/jurnalis-engine.svg)](https://crates.io/crates/jurnalis-engine)
  [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
</div>

A stateless, deterministic text-based CRPG engine implementing SRD 5.1 (d20) mechanics. The engine is a standalone Rust library crate — it carries no server, no persistent state, and no runtime dependencies beyond `serde` and `rand`. Embed it in any application or run it directly via the included `jurnalis-cli` binary.

All game state is serialized to JSON and owned by the caller. On every call the caller passes the current state in; the engine returns the updated state alongside text output. This makes the engine trivially embeddable in web backends, desktop apps, and test harnesses alike.

---

## Features

- **Full SRD 5.1 mechanics** — d20 ability checks, saving throws, skill checks, initiative, combat turns with attack rolls and damage, spell slots, short and long rests, conditions, and AC calculations.
- **12 character classes** — Barbarian, Bard, Cleric, Druid, Fighter, Monk, Paladin, Ranger, Rogue, Sorcerer, Warlock, Wizard.
- **3 playable races** — Human, Elf, Dwarf, each with accurate racial trait bonuses.
- **Stateless design** — the engine holds zero mutable state. State is serialized to JSON after every call and passed back in on the next. Safe to use across threads, processes, or network boundaries.
- **Deterministic RNG** — seeded from a `u64`. Replay any session exactly by replaying the seed and input sequence.
- **Ironman mode** — optional flag that gates save/load semantics at the application layer.
- **Procedural world generation** — location graphs, NPC placement, item tables, and event triggers generated from the seed at game start.
- **`jurnalis-cli` binary** — a REPL that runs a full game session in a terminal, with built-in `save`/`load` persistence.

---

## Getting Started

### As a library

Add the crate to your `Cargo.toml`:

```toml
[dependencies]
jurnalis-engine = "0.19"
```

### Running the CLI

Build and run the `jurnalis-cli` binary directly from source:

```bash
git clone https://github.com/jurnalis-project/engine.git
cd engine
cargo run --bin jurnalis-cli
```

Or install it globally:

```bash
cargo install jurnalis-engine --bin jurnalis-cli
```

---

## Public API

The engine exposes two functions.

### `new_game`

```rust
pub fn new_game(seed: u64, ironman_mode: bool) -> GameOutput
```

Initializes a new game session and begins the character creation flow. The `seed` value controls all procedural generation for the session. Pass `ironman_mode: true` to signal to your application layer that save/load should be restricted.

### `process_input`

```rust
pub fn process_input(state_json: &str, input: &str) -> GameOutput
```

Accepts the current serialized game state and a raw text command from the player. Returns the updated state and any text to display. The input string is free-form — the engine's parser handles verb resolution, fuzzy target matching, and dispatch to the appropriate subsystem.

### `GameOutput`

```rust
pub struct GameOutput {
    pub text: Vec<String>,      // Lines of text to display to the player
    pub state_json: String,     // Updated game state, serialized as JSON
    pub state_changed: bool,    // Whether state was mutated by this call
}
```

---

## Usage Example

```rust
use jurnalis_engine::{new_game, process_input};

fn main() {
    // Start a new game with a fixed seed (deterministic)
    let output = new_game(42, false);
    for line in &output.text {
        println!("{}", line);
    }

    // The caller owns the state — pass it back on every call
    let mut state_json = output.state_json;

    // Drive the game loop
    let commands = ["1", "3", "2", "15 14 13 12 10 8", "1 2", "Aria", "look", "north"];
    for cmd in &commands {
        let output = process_input(&state_json, cmd);
        for line in &output.text {
            println!("{}", line);
        }
        state_json = output.state_json;
    }
}
```

The caller is responsible for storing `state_json` between calls. There is no hidden global state.

---

## Architecture

The crate is organized into feature modules. Each module owns its domain logic and exposes a public API; modules do not import from each other directly. `lib.rs` is the sole orchestrator — it calls into modules, routes commands, and threads state through the pipeline.

```
src/
  lib.rs          # Orchestrator: new_game(), process_input()
  types.rs        # Shared enums and structs (Ability, Skill, Direction, …)
  state/          # GameState, WorldState, serialization, save/load validation
  parser/         # Command parsing, verb dispatch, fuzzy target resolution
  character/      # Character creation, ability scores, races, classes, backgrounds
  combat/         # Initiative, attack rolls, damage, turn order
  conditions/     # Status conditions (Blinded, Poisoned, Restrained, …)
  equipment/      # SRD item tables, AC calculation, equip/unequip
  leveling/       # XP tracking, level-up, proficiency bonus progression
  rest/           # Short rest (hit dice recovery) and long rest (full recovery)
  rules/          # Dice rolling (seeded RNG), skill checks, saving throws
  spells/         # Spell slot management, spell casting, known/prepared spells
  world/          # Procedural location graph, NPC generation, item placement, triggers
  narration/      # Template-based narrator, event-to-text rendering
  output/         # GameOutput struct
```

Shared data structures (`GameState`, `Ability`, `Skill`, `ItemType`, etc.) live in `state/` and `types.rs` so every module can depend on them without creating cross-module coupling.

---

## Contributing

Contributions are welcome. Please open an issue to discuss significant changes before sending a pull request.

1. Fork the repository and create a branch off `main`.
2. Make your changes. Run the test suite before pushing:
   ```bash
   cargo test
   ```
3. Ensure no new warnings are introduced:
   ```bash
   cargo clippy -- -D warnings
   ```
4. Open a pull request against `main` with a clear description of what changed and why.

There is no minimum issue size — bug reports, documentation fixes, and new mechanics are all appreciated.

---

## License

This project does not yet have a license file committed to the repository. Until one is added, all rights are reserved. If you want to use this crate in your project, please open an issue to request a license.
