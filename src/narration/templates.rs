use rand::Rng;

pub fn pick<'a>(rng: &mut impl Rng, options: &'a [&str]) -> &'a str {
    options[rng.gen_range(0..options.len())]
}

pub const ENTER_LOCATION: &[&str] = &[
    "You enter {name}. {description}",
    "You step into {name}. {description}",
    "Before you lies {name}. {description}",
];

pub const LOOK_LOCATION: &[&str] = &[
    "You are in {name}. {description}",
    "{name}. {description}",
];

pub const EXITS: &str = "Exits: {exits}.";
pub const NPCS_PRESENT: &str = "You see {npcs} here.";
pub const ITEMS_PRESENT: &str = "On the ground: {items}.";

pub const SKILL_CHECK_SUCCESS: &[&str] = &[
    "[{skill} check: {roll}+{mod}={total} vs DC {dc} — Success!]",
];

pub const SKILL_CHECK_FAILURE: &[&str] = &[
    "[{skill} check: {roll}+{mod}={total} vs DC {dc} — Failure.]",
];

pub const SAVE_SUCCESS: &[&str] = &[
    "[{ability} save: {roll}+{mod}={total} vs DC {dc} — Success!]",
];

pub const SAVE_FAILURE: &[&str] = &[
    "[{ability} save: {roll}+{mod}={total} vs DC {dc} — Failure.]",
];

pub const TAKE_ITEM: &str = "You pick up the {item}.";
pub const ITEM_NOT_FOUND: &str = "You don't see any \"{item}\" here.";
pub const NPC_NOT_FOUND: &str = "There's no one called \"{name}\" here.";
pub const NO_EXIT: &str = "You can't go {direction}.";
pub const UNKNOWN_COMMAND: &str = "I don't understand \"{input}\". Type 'help' for commands.";
pub const EMPTY_INVENTORY: &str = "You aren't carrying anything.";

pub const HELP_TEXT: &str = "\
Commands:
  look [target]     - Examine surroundings or a specific thing
  go <direction>    - Move (or use n/s/e/w/u/d)
  talk <npc>        - Talk to someone
  take <item>       - Pick up an item
  use <item>        - Use an item
  inventory (i)     - Check your inventory
  character (char)  - View character sheet
  check <skill>     - Attempt a skill check
  save [name]       - Save game
  load [name]       - Load game
  help [command]    - Show this help";
