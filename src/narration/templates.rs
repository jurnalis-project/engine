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
pub const DROP_ITEM: &str = "You drop the {item}.";
pub const ITEM_NOT_FOUND: &str = "You don't see any \"{item}\" here.";
pub const NPC_NOT_FOUND: &str = "There's no one called \"{name}\" here.";
pub const NO_EXIT: &str = "You can't go {direction}.";
pub const UNKNOWN_COMMAND: &str = "I don't understand \"{input}\". Type 'help' for commands.";
pub const EMPTY_INVENTORY: &str = "You aren't carrying anything.";

pub const EQUIP_WIELD: &str = "You wield the {item}.";
pub const EQUIP_WIELD_OFF: &str = "You wield the {item} in your off hand.";
pub const EQUIP_WEAR: &str = "You put on the {item}.";
pub const EQUIP_SHIELD: &str = "You strap on the {item}.";
pub const EQUIP_SWAP_WEAPON: &str = "You put away the {old} and wield the {new}.";
pub const EQUIP_SWAP_ARMOR: &str = "You remove the {old} and put on the {new}.";
pub const EQUIP_TWO_HAND_CLEAR: &str = "You put away the {offhand} and wield the {weapon} with both hands.";
pub const EQUIP_NOT_FOUND: &str = "You don't have any \"{name}\".";
pub const EQUIP_CANT: &str = "You can't equip the {item}.";
pub const UNEQUIP_WEAPON: &str = "You put away the {item}.";
pub const UNEQUIP_ARMOR: &str = "You remove the {item}.";
pub const UNEQUIP_NOT_EQUIPPED: &str = "You don't have \"{name}\" equipped.";

pub const HELP_TEXT: &str = "\
Commands:
  look [target]     - Examine surroundings or a specific thing
                      (also: examine, inspect, see, search, l)
  go <direction>    - Move (or use n/s/e/w/u/d)
                      (also: walk, move, head)
  talk <npc>        - Talk to someone
                      (also: talk to, speak, speak to, ask)
  take <item>       - Pick up an item
                      (also: get, grab, pick up, collect)
  drop <item>       - Drop an item from your inventory
                      (also: put down, discard)
  equip <item>      - Equip a weapon or armor
                      (also: wear, wield, don, put on)
  unequip <item>    - Remove equipped gear
                      (also: doff, take off)
  use <item>        - Use an item
                      (also: activate, apply)
  inventory (i)     - Check your inventory
                      (also: inv, items, bag)
  character (char)  - View character sheet
                      (also: sheet, stats, status)
  check <skill>     - Attempt a skill check
                      (also: roll, try)
  save [name]       - Save game
  load [name]       - Load game (also: restore)
  help              - Show this help (also: ?, commands)";
