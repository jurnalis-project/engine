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

// Consumable effect templates
pub const USE_HEAL: &str = "You drink the {item}. You recover {roll} HP. (HP: {current}/{max})";
pub const USE_HEAL_FULL: &str = "You drink the {item}. You feel refreshed, but you're already at full health. (HP: {current}/{max})";
pub const USE_LIGHT_UPGRADE: &str = "You light the {item}. The room brightens from {old_level} to {new_level}.";
pub const USE_LIGHT_ALREADY_BRIGHT: &str = "You light the {item}, but the room is already brightly lit.";
pub const USE_NOURISH: &str = "You eat the {item}. You feel nourished and ready for the journey ahead.";
pub const USE_UNKNOWN_EFFECT: &str = "You use the {item}. Nothing happens.";
pub const USE_NOT_CONSUMABLE: &str = "You can't use the {item} that way.";

// -- Spell templates --
pub const CAST_NOT_A_CASTER: &str = "You don't know any spells.";
pub const CAST_UNKNOWN_SPELL: &str = "You don't know that spell.";
pub const CAST_NO_SLOTS: &str = "You have no spell slots remaining.";
pub const CAST_NOT_IN_COMBAT: &str = "You can only cast that spell in combat.";
pub const CAST_NEED_TARGET: &str = "Cast {spell} at whom?";
pub const CAST_PRESTIDIGITATION: &str = "You snap your fingers and a cascade of harmless sparks dances across your palm.";
pub const CAST_FIRE_BOLT_HIT: &str = "You hurl a bolt of fire at {target} ({roll}+{mod}={total} vs AC {ac}) -- hit for {damage} fire damage!";
pub const CAST_FIRE_BOLT_CRIT: &str = "You hurl a bolt of fire at {target} -- CRITICAL HIT! {damage} fire damage!";
pub const CAST_FIRE_BOLT_MISS: &str = "You hurl a bolt of fire at {target} ({roll}+{mod}={total} vs AC {ac}) -- the bolt flies wide.";
pub const CAST_FIRE_BOLT_MISS_NAT1: &str = "You hurl a bolt of fire at {target} -- natural 1! The bolt fizzles.";
pub const CAST_FIRE_BOLT_EXPLORE: &str = "You conjure a mote of fire, but there's nothing to throw it at.";
pub const CAST_MAGIC_MISSILE: &str = "Three glowing darts of force streak toward {target}, dealing {d1}, {d2}, and {d3} damage ({total} total force damage).";
pub const CAST_BURNING_HANDS_INTRO: &str = "Flames shoot from your outstretched fingers! (3d6 = {damage} fire, DC {dc} DEX save)";
pub const CAST_BURNING_HANDS_FAIL: &str = "  {target}: {save_result} -- takes {damage} fire damage!";
pub const CAST_BURNING_HANDS_SAVE: &str = "  {target}: {save_result} -- takes {damage} fire damage (half).";
pub const CAST_BURNING_HANDS_NO_TARGETS: &str = "You release a fan of flames, but no enemies are close enough.";
pub const CAST_SLEEP_INTRO: &str = "A wave of magical drowsiness rolls out (5d8 = {pool} HP affected).";
pub const CAST_SLEEP_TARGET: &str = "  {target} ({hp} HP) falls asleep!";
pub const CAST_SLEEP_NONE: &str = "  No creatures are affected.";
pub const CAST_SHIELD: &str = "A shimmering barrier of force appears. (+5 AC until your next turn)";
pub const CAST_SLOT_USED: &str = "[Spell slot used: {remaining}/{max} level {level} slots remaining]";

// -- Ritual-cast templates --
pub const CAST_NOT_A_RITUAL: &str = "{spell} doesn't have the Ritual tag — cast it normally.";
pub const CAST_RITUAL_INTRO: &str = "You begin a ritual casting of {spell}. (No spell slot consumed. Takes longer than normal in-world.)";

// -- Concentration templates --
pub const CONCENTRATION_STARTED: &str = "You focus on maintaining {spell}.";
pub const CONCENTRATION_DROPPED: &str = "You release your concentration on {old} to focus on {new}.";
pub const CONCENTRATION_BROKEN: &str = "Your concentration on {spell} is broken!";
pub const CONCENTRATION_HELD: &str = "You grit your teeth and maintain concentration on {spell}.";

// -- Condition templates --
// Placeholders: {target} = creature name or "You", {condition} = lowercase condition name.
// The orchestrator picks the correct variant (self vs other) based on whether the
// affected combatant is the player.
pub const CONDITION_APPLIED_SELF: &str = "You are {condition}!";
pub const CONDITION_APPLIED_OTHER: &str = "{target} is {condition}!";
pub const CONDITION_SAVED_SELF: &str = "You shake off the {condition}.";
pub const CONDITION_SAVED_OTHER: &str = "{target} shakes off the {condition}.";
pub const CONDITION_EXPIRED_SELF: &str = "The {condition} wears off.";
pub const CONDITION_EXPIRED_OTHER: &str = "{target} is no longer {condition}.";

// Exhaustion-specific templates since it tracks a numeric level rather than a
// boolean condition entry.
pub const EXHAUSTION_GAINED_SELF: &str = "You gain a level of exhaustion (now level {level}).";
pub const EXHAUSTION_GAINED_OTHER: &str = "{target} gains a level of exhaustion (now level {level}).";
pub const EXHAUSTION_LETHAL_SELF: &str = "Your exhaustion reaches level 6. You collapse, lifeless.";
pub const EXHAUSTION_LETHAL_OTHER: &str = "{target} collapses, lifeless from exhaustion.";

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
  help              - Show this help (also: ?, commands)

Combat commands (available during combat):
  attack <target>   - Attack an enemy
                      (also: hit, strike, swing at, shoot)
  approach <target> - Move toward an enemy
                      (also: advance, close, move to, move toward)
  retreat           - Move away from all enemies
                      (also: move away, fall back, back up)
  dodge             - Take Dodge action (disadvantage on incoming attacks)
  disengage         - Take Disengage action (no opportunity attacks)
                      (also: withdraw)
  dash              - Take Dash action (double movement)
                      (also: run, sprint)
  end turn          - End your turn (also: end, pass, wait)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpPhase {
    Exploration,
    Combat,
}

const EXPLORATION_HELP_TOPICS: &[&str] = &[
    "movement",
    "interaction",
    "inventory",
    "equipment",
    "checks",
    "spells",
    "system",
    "combat",
];

const COMBAT_HELP_TOPICS: &[&str] = &[
    "movement",
    "inventory",
    "equipment",
    "spells",
    "system",
    "combat",
];

pub fn render_help(topic: Option<&str>, phase: HelpPhase) -> Vec<String> {
    let topic = topic.map(str::trim).filter(|value| !value.is_empty());

    match topic {
        None => overview_help(phase),
        Some(raw_topic) => {
            let Some(canonical_topic) = normalize_help_topic(raw_topic) else {
                return unknown_topic_help(raw_topic, phase);
            };

            if !phase.valid_topics().contains(&canonical_topic) {
                return unavailable_topic_help(canonical_topic, phase);
            }

            topic_help(canonical_topic, phase)
        }
    }
}

impl HelpPhase {
    fn valid_topics(self) -> &'static [&'static str] {
        match self {
            HelpPhase::Exploration => EXPLORATION_HELP_TOPICS,
            HelpPhase::Combat => COMBAT_HELP_TOPICS,
        }
    }

    fn name(self) -> &'static str {
        match self {
            HelpPhase::Exploration => "exploration",
            HelpPhase::Combat => "combat",
        }
    }
}

fn normalize_help_topic(raw_topic: &str) -> Option<&'static str> {
    let normalized = raw_topic.trim().to_lowercase().replace('-', " ");

    match normalized.as_str() {
        "movement" | "move" | "travel" | "navigation" | "directions" => Some("movement"),
        "interaction" | "interact" | "look" | "talk" | "social" => Some("interaction"),
        "inventory" | "inv" | "items" | "bag" => Some("inventory"),
        "equipment" | "equip" | "gear" => Some("equipment"),
        "checks" | "check" | "skill" | "skills" | "roll" => Some("checks"),
        "system" | "save" | "load" | "help" | "commands" => Some("system"),
        "combat" | "battle" | "fight" | "attack" => Some("combat"),
        "spells" | "spell" | "magic" | "cast" | "casting" => Some("spells"),
        _ => None,
    }
}

fn overview_help(phase: HelpPhase) -> Vec<String> {
    match phase {
        HelpPhase::Exploration => vec![
            "Commands overview (exploration):".to_string(),
            format!("Topics: {}.", phase.valid_topics().join(", ")),
            "Type 'help <topic>' for focused guidance.".to_string(),
            "Quick start: look, go <direction>, talk <npc>, take <item>, inventory, character, objective, map.".to_string(),
            "Use 'help combat' to preview commands that unlock during battles.".to_string(),
        ],
        HelpPhase::Combat => vec![
            "Commands overview (combat):".to_string(),
            format!("Topics: {}.", phase.valid_topics().join(", ")),
            "Type 'help <topic>' for focused guidance.".to_string(),
            "Quick start: attack <target>, approach <target>, retreat, dodge, dash, end turn.".to_string(),
            "Utility commands still available: look, inventory, character, equip, unequip, help, objective, map.".to_string(),
        ],
    }
}

fn topic_help(topic: &str, phase: HelpPhase) -> Vec<String> {
    match (topic, phase) {
        ("movement", HelpPhase::Exploration) => vec![
            "Help: movement (exploration)".to_string(),
            "  go <direction> - Move to an adjacent location.".to_string(),
            "  Direction shortcuts: n, s, e, w, u, d.".to_string(),
            "  Aliases: walk, move, head.".to_string(),
        ],
        ("movement", HelpPhase::Combat) => vec![
            "Help: movement (combat)".to_string(),
            "  approach <target> - Move toward an enemy.".to_string(),
            "  retreat - Move away from all enemies.".to_string(),
            "  dash - Double your movement for this turn.".to_string(),
            "  Note: go <direction> is disabled during combat.".to_string(),
        ],
        ("interaction", HelpPhase::Exploration) => vec![
            "Help: interaction".to_string(),
            "  look [target] - Examine the area or a specific target.".to_string(),
            "  talk <npc> - Start dialogue with someone nearby.".to_string(),
            "  take <item> / drop <item> - Move items between room and inventory.".to_string(),
            "  use <item> - Activate consumables or usable items.".to_string(),
        ],
        ("inventory", _) => vec![
            "Help: inventory".to_string(),
            "  inventory (i) - List carried items and equipped tags.".to_string(),
            "  take <item> - Pick up an item into inventory.".to_string(),
            "  drop <item> - Remove an item from inventory.".to_string(),
        ],
        ("equipment", _) => vec![
            "Help: equipment".to_string(),
            "  equip <item> - Equip a weapon or armor piece.".to_string(),
            "  unequip <item> - Remove equipped gear.".to_string(),
            "  Optional suffix: 'off hand' (for light one-handed weapons).".to_string(),
            "  Example: equip dagger off hand".to_string(),
        ],
        ("checks", HelpPhase::Exploration) => vec![
            "Help: checks".to_string(),
            "  check <skill> - Roll a skill check against the default DC.".to_string(),
            "  Aliases: roll, try.".to_string(),
            "  Example: check perception".to_string(),
        ],
        ("system", _) => vec![
            "Help: system".to_string(),
            "  save [name] - Prepare game state for saving (frontend writes file).".to_string(),
            "  load [name] - Load a saved state (frontend reads file).".to_string(),
            "  help / ? / commands - Show overview or topic help.".to_string(),
            "  character (char) - View your character sheet.".to_string(),
        ],
        ("combat", HelpPhase::Exploration) => vec![
            "Help: combat".to_string(),
            "Combat starts automatically when hostile NPCs are present.".to_string(),
            "When combat starts, these commands unlock: attack, approach, retreat, dodge, disengage, dash, end turn.".to_string(),
            "Use 'help combat' again during battle for in-combat details.".to_string(),
        ],
        ("combat", HelpPhase::Combat) => vec![
            "Help: combat".to_string(),
            "  attack <target> - Attack an enemy in range.".to_string(),
            "  approach <target> - Move toward an enemy.".to_string(),
            "  retreat - Move away from all enemies.".to_string(),
            "  dodge / disengage / dash - Tactical actions for your turn.".to_string(),
            "  end turn - End your turn and advance initiative.".to_string(),
            "  Bonus actions (one per turn):".to_string(),
            "    bonus dash / dash as bonus - Dash using your bonus action instead.".to_string(),
            "    offhand attack <target> / attack <target> off hand - Two-Weapon Fighting.".to_string(),
            "  Reaction: when an enemy triggers a reaction (e.g. incoming hit for Shield,".to_string(),
            "    or leaving your melee reach for an opportunity attack), answer 'yes' or 'no'.".to_string(),
        ],
        ("spells", _) => vec![
            "Help: spells".to_string(),
            "  spells            - View your known spells and remaining spell slots.".to_string(),
            "                      (also: spell list, known spells, my spells)".to_string(),
            "  cast <spell> [at <target>] - Cast a spell.".to_string(),
            "  Cantrips (free): Fire Bolt (ranged attack, 1d10 fire), Prestidigitation (flavor).".to_string(),
            "  Level 1 spells (use spell slots): Magic Missile, Burning Hands, Sleep, Shield.".to_string(),
            "  Spell attack: d20 + INT mod + proficiency vs AC.".to_string(),
            "  Spell save DC: 8 + INT mod + proficiency.".to_string(),
            "  Only Wizards can cast spells.".to_string(),
        ],
        _ => unreachable!("Topic '{topic}' should be resolved before rendering"),
    }
}

fn unknown_topic_help(raw_topic: &str, phase: HelpPhase) -> Vec<String> {
    vec![
        format!("Unknown help topic: '{}'.", raw_topic.trim()),
        format!(
            "Valid topics during {}: {}.",
            phase.name(),
            phase.valid_topics().join(", ")
        ),
        "Type 'help' for an overview.".to_string(),
    ]
}

fn unavailable_topic_help(topic: &str, phase: HelpPhase) -> Vec<String> {
    vec![
        format!(
            "The '{}' topic is not available during {}.",
            topic,
            phase.name()
        ),
        format!("Valid topics right now: {}.", phase.valid_topics().join(", ")),
        "Type 'help' for an overview.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{render_help, HelpPhase};

    #[test]
    fn help_overview_lists_topics_for_exploration() {
        let lines = render_help(None, HelpPhase::Exploration);

        assert!(lines.iter().any(|line| line.contains("Commands overview (exploration)")));
        assert!(lines.iter().any(|line| line.contains("movement")));
        assert!(lines.iter().any(|line| line.contains("combat")));
    }

    #[test]
    fn help_topic_is_phase_aware() {
        let exploration_lines = render_help(Some("movement"), HelpPhase::Exploration);
        let combat_lines = render_help(Some("movement"), HelpPhase::Combat);

        assert!(exploration_lines.iter().any(|line| line.contains("go <direction>")));
        assert!(combat_lines.iter().any(|line| line.contains("approach <target>")));
    }

    #[test]
    fn help_unknown_topic_lists_valid_topics_for_phase() {
        let lines = render_help(Some("mystery"), HelpPhase::Combat);

        assert!(lines.iter().any(|line| line.contains("Unknown help topic")));
        assert!(lines.iter().any(|line| line.contains("Valid topics during combat")));
        assert!(lines.iter().any(|line| line.contains("movement, inventory, equipment, spells, system, combat")));
    }
}

