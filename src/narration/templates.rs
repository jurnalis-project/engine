use rand::Rng;

pub fn pick<'a>(rng: &mut impl Rng, options: &'a [&str]) -> &'a str {
    options[rng.gen_range(0..options.len())]
}

pub const ENTER_LOCATION: &[&str] = &[
    "You enter {name}. {description}",
    "You step into {name}. {description}",
    "Before you lies {name}. {description}",
];

pub const LOOK_LOCATION: &[&str] = &["You are in {name}. {description}", "{name}. {description}"];

pub const EXITS: &str = "Exits: {exits}.";
pub const NPCS_PRESENT: &str = "You see {npcs} here.";
pub const ITEMS_PRESENT: &str = "On the ground: {items}.";

pub const SKILL_CHECK_SUCCESS: &[&str] =
    &["[{skill} check: {roll}+{mod}={total} vs DC {dc} — Success!]"];

pub const SKILL_CHECK_FAILURE: &[&str] =
    &["[{skill} check: {roll}+{mod}={total} vs DC {dc} — Failure.]"];

pub const SAVE_SUCCESS: &[&str] = &["[{ability} save: {roll}+{mod}={total} vs DC {dc} — Success!]"];

pub const SAVE_FAILURE: &[&str] = &["[{ability} save: {roll}+{mod}={total} vs DC {dc} — Failure.]"];

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
pub const EQUIP_TWO_HAND_CLEAR: &str =
    "You put away the {offhand} and wield the {weapon} with both hands.";
pub const EQUIP_NOT_FOUND: &str = "You don't have any \"{name}\".";
pub const EQUIP_CANT: &str = "You can't equip the {item}.";
pub const UNEQUIP_WEAPON: &str = "You put away the {item}.";
pub const UNEQUIP_ARMOR: &str = "You remove the {item}.";
pub const UNEQUIP_NOT_EQUIPPED: &str = "You don't have \"{name}\" equipped.";

// Consumable effect templates
pub const USE_HEAL: &str = "You drink the {item}. You recover {roll} HP. (HP: {current}/{max})";
pub const USE_HEAL_FULL: &str = "You drink the {item}. You feel refreshed, but you're already at full health. (HP: {current}/{max})";
pub const USE_LIGHT_UPGRADE: &str =
    "You light the {item}. The room brightens from {old_level} to {new_level}.";
pub const USE_LIGHT_ALREADY_BRIGHT: &str =
    "You light the {item}, but the room is already brightly lit.";
pub const USE_NOURISH: &str =
    "You eat the {item}. You feel nourished and ready for the journey ahead.";
pub const USE_UNKNOWN_EFFECT: &str = "You use the {item}. Nothing happens.";
pub const USE_NOT_CONSUMABLE: &str = "You can't use the {item} that way.";

// -- Spell templates --
pub const CAST_NOT_A_CASTER: &str = "You don't know any spells.";
pub const CAST_UNKNOWN_SPELL: &str = "You don't know that spell.";
pub const CAST_NO_SLOTS: &str = "You have no spell slots remaining.";
pub const CAST_NOT_IN_COMBAT: &str = "You can only cast that spell in combat.";
pub const CAST_NEED_TARGET: &str = "Cast {spell} at whom?";
pub const CAST_PRESTIDIGITATION: &str =
    "You snap your fingers and a cascade of harmless sparks dances across your palm.";
pub const CAST_FIRE_BOLT_HIT: &str = "You hurl a bolt of fire at {target} ({roll}+{mod}={total} vs AC {ac}) -- hit for {damage} fire damage!";
pub const CAST_FIRE_BOLT_CRIT: &str =
    "You hurl a bolt of fire at {target} -- CRITICAL HIT! {damage} fire damage!";
pub const CAST_FIRE_BOLT_MISS: &str =
    "You hurl a bolt of fire at {target} ({roll}+{mod}={total} vs AC {ac}) -- the bolt flies wide.";
pub const CAST_FIRE_BOLT_MISS_NAT1: &str =
    "You hurl a bolt of fire at {target} -- natural 1! The bolt fizzles.";
pub const CAST_FIRE_BOLT_EXPLORE: &str =
    "You conjure a mote of fire, but there's nothing to throw it at.";
pub const CAST_MAGIC_MISSILE: &str = "Three glowing darts of force streak toward {target}, dealing {d1}, {d2}, and {d3} damage ({total} total force damage).";
pub const CAST_BURNING_HANDS_INTRO: &str =
    "Flames shoot from your outstretched fingers! (3d6 = {damage} fire, DC {dc} DEX save)";
pub const CAST_BURNING_HANDS_FAIL: &str =
    "  {target}: {save_result} -- takes {damage} fire damage!";
pub const CAST_BURNING_HANDS_SAVE: &str =
    "  {target}: {save_result} -- takes {damage} fire damage (half).";
pub const CAST_BURNING_HANDS_NO_TARGETS: &str =
    "You release a fan of flames, but no enemies are close enough.";
pub const CAST_SLEEP_INTRO: &str =
    "A wave of magical drowsiness rolls out (5d8 = {pool} HP affected).";
pub const CAST_SLEEP_TARGET: &str = "  {target} ({hp} HP) falls asleep!";
pub const CAST_SLEEP_NONE: &str = "  No creatures are affected.";
pub const CAST_SHIELD: &str = "A shimmering barrier of force appears. (+5 AC until your next turn)";
pub const CAST_REACTION_ONLY: &str = "{spell} can only be cast as a reaction. It triggers automatically when you are hit by an attack or targeted by Magic Missile.";
pub const CAST_SLOT_USED: &str =
    "[Spell slot used: {remaining}/{max} level {level} slots remaining]";

// -- Non-Wizard combat cantrips and spells --
pub const CAST_SACRED_FLAME_HIT: &str =
    "Radiant flames rain down on {target} -- {save_result} -- takes {damage} radiant damage!";
pub const CAST_SACRED_FLAME_SAVE: &str =
    "Radiant flames rain down on {target} -- {save_result} -- they sidestep the light.";
pub const CAST_VICIOUS_MOCKERY_HIT: &str = "You hurl a venomous insult at {target} -- {save_result} -- it stings for {damage} psychic damage!";
pub const CAST_VICIOUS_MOCKERY_SAVE: &str =
    "You hurl a venomous insult at {target} -- {save_result} -- {target} shrugs it off.";
pub const CAST_ELDRITCH_BLAST_HIT: &str = "A crackling beam of eldritch energy lances toward {target} ({roll}+{mod}={total} vs AC {ac}) -- hit for {damage} force damage!";
pub const CAST_ELDRITCH_BLAST_CRIT: &str = "A crackling beam of eldritch energy lances toward {target} -- CRITICAL HIT! {damage} force damage!";
pub const CAST_ELDRITCH_BLAST_MISS: &str = "A crackling beam of eldritch energy lances toward {target} ({roll}+{mod}={total} vs AC {ac}) -- the beam goes wide.";
pub const CAST_ELDRITCH_BLAST_MISS_NAT1: &str =
    "A crackling beam of eldritch energy lances toward {target} -- natural 1! The beam dissipates.";
pub const CAST_GUIDING_BOLT_HIT: &str = "A flash of radiant light streaks at {target} ({roll}+{mod}={total} vs AC {ac}) -- hit for {damage} radiant damage!";
pub const CAST_GUIDING_BOLT_CRIT: &str =
    "A flash of radiant light streaks at {target} -- CRITICAL HIT! {damage} radiant damage!";
pub const CAST_GUIDING_BOLT_MISS: &str = "A flash of radiant light streaks at {target} ({roll}+{mod}={total} vs AC {ac}) -- the light fades before hitting.";
pub const CAST_GUIDING_BOLT_MISS_NAT1: &str =
    "A flash of radiant light streaks at {target} -- natural 1! The bolt sputters out.";
pub const CAST_CURE_WOUNDS_SELF: &str = "Your hands glow with a warm light. You recover {roll}+{mod}={healing} HP. (HP: {current}/{max})";
pub const CAST_HEALING_WORD_SELF: &str =
    "You speak a word of healing. You recover {roll}+{mod}={healing} HP. (HP: {current}/{max})";
pub const CAST_HEAL_FULL_HP: &str =
    "You cast {spell}, but you are already at full health. (HP: {current}/{max})";
pub const CAST_BLESS: &str =
    "You call upon divine favor. Your allies are blessed with radiant resolve.";
pub const CAST_CHARM_PERSON_HIT: &str = "You whisper words of charm to {target} -- {save_result} -- they now regard you as a friendly acquaintance.";
pub const CAST_CHARM_PERSON_SAVE: &str =
    "You whisper words of charm to {target} -- {save_result} -- they shrug off the enchantment.";
pub const CAST_FAERIE_FIRE_HIT: &str =
    "Motes of faerie fire outline {target} -- {save_result} -- they glow with harmless light!";
pub const CAST_FAERIE_FIRE_SAVE: &str =
    "Motes of faerie fire outline {target} -- {save_result} -- they evade the glow.";

// -- Flavor cantrips (exploration/combat utility) --
pub const CAST_DRUIDCRAFT: &str =
    "You weave a tiny nature-magic flourish -- a bud blooms in your palm.";
pub const CAST_MAGE_HAND: &str =
    "A spectral, glowing hand appears beside you, ready to manipulate small objects nearby.";
pub const CAST_LIGHT: &str =
    "You touch the nearest object and it sheds bright light in a 20-foot radius.";
pub const CAST_GUIDANCE: &str = "You place a hand on your own shoulder. A subtle glow settles around you, ready to aid a check.";
pub const CAST_MINOR_ILLUSION: &str =
    "You conjure a small illusion -- a flicker, a whisper, a shadow that isn't there.";
pub const CAST_MAGE_ARMOR: &str = "A protective weave of force settles around you. Your base AC is now 13 + your Dexterity modifier while you remain unarmored.";

// -- Ritual-cast templates --
pub const CAST_NOT_A_RITUAL: &str = "{spell} doesn't have the Ritual tag — cast it normally.";
pub const CAST_RITUAL_INTRO: &str = "You begin a ritual casting of {spell}. (No spell slot consumed. Takes longer than normal in-world.)";

// -- Concentration templates --
pub const CONCENTRATION_STARTED: &str = "You focus on maintaining {spell}.";
pub const CONCENTRATION_DROPPED: &str =
    "You release your concentration on {old} to focus on {new}.";
pub const CONCENTRATION_BROKEN: &str = "Your concentration on {spell} is broken!";
pub const CONCENTRATION_HELD: &str = "You grit your teeth and maintain concentration on {spell}.";

// -- Divine Smite templates --
pub const DIVINE_SMITE_HIT: &str = "Divine Smite: +{damage} radiant damage ({dice}d8).";
pub const DIVINE_SMITE_HIT_EXTRA: &str =
    "Divine Smite: +{damage} radiant damage ({dice}d8 -- extra radiance against {creature_type}).";
pub const DIVINE_SMITE_FREE: &str = "Divine Smite: +{damage} radiant damage ({dice}d8). (free use)";
pub const DIVINE_SMITE_FREE_EXTRA: &str =
    "Divine Smite: +{damage} radiant damage ({dice}d8 -- extra radiance against {creature_type}). (free use)";

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
pub const EXHAUSTION_GAINED_OTHER: &str =
    "{target} gains a level of exhaustion (now level {level}).";
pub const EXHAUSTION_LETHAL_SELF: &str = "Your exhaustion reaches level 6. You collapse, lifeless.";
pub const EXHAUSTION_LETHAL_OTHER: &str = "{target} collapses, lifeless from exhaustion.";

// -- Turn-end templates --
pub const END_TURN: &str = "You end your turn.";
pub const END_TURN_WAIT: &str = "You wait, letting your turn pass without acting.";

// -- NPC ambient bark templates (one-liners per NpcRole) --
pub const BARK_GUARD: &[&str] = &[
    "\"Keep moving.\"",
    "\"Stay out of trouble.\"",
    "\"I've got my eye on you.\"",
];

pub const BARK_MERCHANT: &[&str] = &[
    "\"Fine goods here, if you're buying.\"",
    "\"Best prices this side of the dungeon.\"",
    "\"Take a look, friend. You won't be disappointed.\"",
];

pub const BARK_HERMIT: &[&str] = &[
    "\"The walls remember what you forget...\"",
    "\"Shh. Listen. Do you hear it?\"",
    "\"Few come this way. Fewer leave.\"",
];

pub const BARK_ADVENTURER: &[&str] = &[
    "\"Careful ahead -- I've seen things.\"",
    "\"Another explorer? Good luck to you.\"",
    "\"Treasure's deeper in, if you dare.\"",
];

pub const HELP_TEXT: &str = "\
Commands:
  look [target]     - Examine surroundings or a specific thing
                      (also: examine, inspect, see, l)
  search [target]   - Search the room for discoveries or inspect a target
                      (also: search for)
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
  attack <target>   - Attack an enemy (also: hit, strike, swing at)
  shoot <target>    - Ranged attack with bow/crossbow (also: fire)
  throw <target>    - Throw a weapon at target (also: hurl, toss, lob)
  approach <target> - Move toward an enemy
                      (also: advance, close, move to, move toward)
  retreat           - Move away from all enemies
                      (also: move away, fall back, back up)
  dodge             - Take Dodge action (disadvantage on incoming attacks)
  disengage         - Take Disengage action (no opportunity attacks)
                      (also: withdraw, flee)
  bonus disengage   - Disengage as bonus action (Rogue Cunning Action)
                      (also: disengage as bonus, cunning disengage)
  dash              - Take Dash action (double movement)
                      (also: run, sprint)
  end turn          - End your turn (also: end, pass, wait)";

// ---- Environment interaction templates ----
// Door
pub const DOOR_OPEN: &str = "You push open the {name}.";
pub const DOOR_ALREADY_OPEN: &str = "The {name} is already open.";
pub const DOOR_LOCKED: &str = "The {name} is locked.";
pub const DOOR_CLOSE: &str = "You pull the {name} closed.";
pub const DOOR_ALREADY_CLOSED: &str = "The {name} is already closed.";
pub const DOOR_UNLOCK: &str = "You unlock the {name}.";
pub const DOOR_NOT_LOCKED: &str = "The {name} is not locked.";
pub const DOOR_FORCE_SUCCESS: &str =
    "With a mighty heave, you force the {name} open. {check}";
pub const DOOR_FORCE_FAIL: &str =
    "You throw your weight against the {name}, but it holds firm. {check}";
pub const DOOR_FORCE_ALREADY_OPEN: &str = "The {name} is already open.";

// Chest
pub const CHEST_OPEN: &str = "You lift the lid of the {name}.";
pub const CHEST_ALREADY_OPEN: &str = "The {name} is already open.";
pub const CHEST_LOCKED: &str = "The {name} is locked.";
pub const CHEST_CLOSE: &str = "You close the lid of the {name}.";
pub const CHEST_ALREADY_CLOSED: &str = "The {name} is already closed.";
pub const CHEST_UNLOCK: &str = "You unlock the {name}.";
pub const CHEST_NOT_LOCKED: &str = "The {name} is not locked.";
pub const CHEST_FORCE_SUCCESS: &str =
    "You wrench the {name} open by force. {check}";
pub const CHEST_FORCE_FAIL: &str =
    "You strain against the {name}, but it won't budge. {check}";

// Lever
pub const LEVER_TOGGLE: &str =
    "You {verb} the {name}. It clicks into a new position.";
pub const LEVER_WRONG_VERB: &str =
    "The {name} doesn't work that way. Try push, pull, or press.";

// Climb
pub const CLIMB_SUCCESS: &str = "You climb the {name} with ease. {check}";
pub const CLIMB_FAIL: &str =
    "You try to climb the {name}, but lose your grip and slide back down. {check}";
pub const CLIMB_NOT_CLIMBABLE: &str = "The {name} is not something you can climb.";

// Generic wrong-verb
pub const FEATURE_WRONG_VERB: &str =
    "You can't {verb} the {name}.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpPhase {
    CharacterCreation,
    Exploration,
    Combat,
}

const CREATION_HELP_TOPICS: &[&str] = &[
    "race",
    "class",
    "abilities",
    "checks",
    "background",
];

const EXPLORATION_HELP_TOPICS: &[&str] = &[
    "movement",
    "actions",
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
    "actions",
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
            HelpPhase::CharacterCreation => CREATION_HELP_TOPICS,
            HelpPhase::Exploration => EXPLORATION_HELP_TOPICS,
            HelpPhase::Combat => COMBAT_HELP_TOPICS,
        }
    }

    fn name(self) -> &'static str {
        match self {
            HelpPhase::CharacterCreation => "character creation",
            HelpPhase::Exploration => "exploration",
            HelpPhase::Combat => "combat",
        }
    }
}

fn normalize_help_topic(raw_topic: &str) -> Option<&'static str> {
    let normalized = raw_topic.trim().to_lowercase().replace('-', " ");

    match normalized.as_str() {
        "movement" | "move" | "travel" | "navigation" | "directions" => Some("movement"),
        "actions" | "action" | "economy" | "turn" | "turns" => Some("actions"),
        "interaction" | "interact" | "look" | "talk" | "social" => Some("interaction"),
        "inventory" | "inv" | "items" | "bag" => Some("inventory"),
        "equipment" | "equip" | "gear" => Some("equipment"),
        "checks" | "check" | "skill" | "skills" | "roll" => Some("checks"),
        "system" | "save" | "load" | "help" | "commands" => Some("system"),
        "combat" | "battle" | "fight" | "attack" => Some("combat"),
        "spells" | "spell" | "magic" | "cast" | "casting" => Some("spells"),
        // Character creation topics
        "race" | "races" | "species" => Some("race"),
        "class" | "classes" => Some("class"),
        "abilities" | "ability" | "ability scores" | "scores" => Some("abilities"),
        "background" | "backgrounds" | "origin" => Some("background"),
        _ => None,
    }
}

fn overview_help(phase: HelpPhase) -> Vec<String> {
    match phase {
        HelpPhase::CharacterCreation => vec![
            "Help (character creation):".to_string(),
            format!("Topics: {}.", phase.valid_topics().join(", ")),
            "Type 'help <topic>' for details on each step.".to_string(),
            "Steps: choose race, class, background, ability scores, skills, alignment, and name.".to_string(),
            "Enter a number or type a name to make your selection at each step.".to_string(),
        ],
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
            "Quick start: attack <target>, approach <target>, retreat, dodge, dash, cast <spell>, end turn.".to_string(),
            "Utility commands still available: look, inventory, character, spells, equip, unequip, help, objective, map.".to_string(),
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
        ("actions", _) => vec![
            "Help: actions".to_string(),
            "  Each turn tracks movement plus four action-economy resources: action, bonus action,".to_string(),
            "  reaction, and free interaction.".to_string(),
            "  Most weapon attacks, grapple, shove, shove prone, dodge, disengage, dash, and rage use your action.".to_string(),
            "  Offhand attack, bonus dash, and bonus disengage use your bonus action when available.".to_string(),
            "  Reactions are prompted mid-round; answer 'yes' or 'no' when one triggers.".to_string(),
            "  If you're a Barbarian, watch for rage, grapple, and shove cues in combat prompts.".to_string(),
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
            "When combat starts, these commands unlock: attack, shoot, throw, approach, retreat, dodge, disengage, dash, end turn.".to_string(),
            "Use 'help actions' for action/bonus/reaction guidance and signature options like rage, grapple, and shove.".to_string(),
            "Spellcasters can also use: cast <spell> [at <target>], spells (to view available spells/slots).".to_string(),
            "Use 'help combat' again during battle for in-combat details.".to_string(),
        ],
        ("combat", HelpPhase::Combat) => vec![
            "Help: combat".to_string(),
            "  attack <target> - Attack an enemy in range.".to_string(),
            "  shoot <target> - Ranged attack with bow/crossbow (also: fire).".to_string(),
            "  throw <target> - Throw a weapon at target (also: hurl, toss, lob).".to_string(),
            "  approach <target> - Move toward an enemy.".to_string(),
            "  retreat - Move away from all enemies.".to_string(),
            "  grapple <target> / shove <target> / shove prone <target> - Strength-based melee control options.".to_string(),
            "  dodge / disengage / dash / rage - Tactical actions for your turn.".to_string(),
            "  cast <spell> [at <target>] - Cast a spell (if you have spells).".to_string(),
            "  spells - View your prepared spells and remaining slots.".to_string(),
            "  end turn - End your turn and advance initiative.".to_string(),
            "    (also: wait, pass - skip your turn without acting)".to_string(),
            "  Bonus actions (one per turn):".to_string(),
            "    bonus dash / dash as bonus - Dash using your bonus action instead.".to_string(),
            "    bonus disengage / disengage as bonus - Disengage using your bonus action (Rogue only).".to_string(),
            "    offhand attack <target> / attack <target> off hand - Two-Weapon Fighting.".to_string(),
            "  Reaction: when an enemy triggers a reaction (e.g. incoming hit for Shield,".to_string(),
            "    or leaving your melee reach for an opportunity attack), answer 'yes' or 'no'.".to_string(),
        ],
        ("spells", _) => vec![
            "Help: spells".to_string(),
            "  spells            - View your known spells and remaining spell slots.".to_string(),
            "                      (also: spell list, known spells, my spells)".to_string(),
            "  cast <spell> [at <target>]  - Cast a spell.".to_string(),
            "  cast <spell> ritual         - Cast a ritual-tagged spell without a slot.".to_string(),
            "  Cantrips (free): attacks, saves, or utility effects -- never consume slots.".to_string(),
            "  Leveled spells (1-3): consume a matching spell slot; use 'spells' to see slot counts.".to_string(),
            "  Spell attack: d20 + your spellcasting ability mod + proficiency vs AC.".to_string(),
            "  Spell save DC: 8 + your spellcasting ability mod + proficiency.".to_string(),
            "  Your spellcasting ability depends on class (e.g. Cleric/Druid use WIS,".to_string(),
            "  Bard/Sorcerer/Warlock use CHA, Wizard uses INT).".to_string(),
        ],
        // ---- Character creation topics ----
        ("race", HelpPhase::CharacterCreation) => vec![
            "Help: race (character creation)".to_string(),
            "  Your race determines ability score bonuses, traits, and starting speed.".to_string(),
            "  Available races: Human, Elf, Dwarf, Dragonborn, Gnome, Goliath, Halfling, Orc, Tiefling.".to_string(),
            "  Some races (Elf, Dragonborn, Gnome, Goliath, Tiefling) have subraces to choose after.".to_string(),
            "  Enter a number or type the race name to select.".to_string(),
        ],
        ("class", HelpPhase::CharacterCreation) => vec![
            "Help: class (character creation)".to_string(),
            "  Your class determines hit dice, saving throws, proficiencies, and features.".to_string(),
            "  12 classes available: Barbarian, Bard, Cleric, Druid, Fighter, Monk,".to_string(),
            "  Paladin, Ranger, Rogue, Sorcerer, Warlock, Wizard.".to_string(),
            "  Enter a number or type the class name to select.".to_string(),
        ],
        ("abilities", HelpPhase::CharacterCreation) => vec![
            "Help: abilities (character creation)".to_string(),
            "  Six ability scores define your character: STR, DEX, CON, INT, WIS, CHA.".to_string(),
            "  Methods: Standard Array (15,14,13,12,10,8), Random (4d6 drop lowest),".to_string(),
            "  or Point Buy (27 points, scores 8-15).".to_string(),
            "  After generating scores, assign them to abilities in order: STR DEX CON INT WIS CHA.".to_string(),
        ],
        ("checks", HelpPhase::CharacterCreation) => vec![
            "Help: skills (character creation)".to_string(),
            "  Your class determines which skill proficiencies you can choose from.".to_string(),
            "  Proficiency adds your proficiency bonus (+2 at level 1) to skill checks.".to_string(),
            "  Enter the numbers of your chosen skills separated by spaces.".to_string(),
        ],
        ("background", HelpPhase::CharacterCreation) => vec![
            "Help: background (character creation)".to_string(),
            "  Your background grants an origin feat and ability score adjustments.".to_string(),
            "  Each background suggests a default origin feat you can accept or change.".to_string(),
            "  After choosing a background, pick an ability adjustment pattern: +2/+1 or +1/+1/+1.".to_string(),
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
        format!(
            "Valid topics right now: {}.",
            phase.valid_topics().join(", ")
        ),
        "Type 'help' for an overview.".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        render_help, HelpPhase, BARK_ADVENTURER, BARK_GUARD, BARK_HERMIT, BARK_MERCHANT,
    };

    #[test]
    fn help_overview_lists_topics_for_exploration() {
        let lines = render_help(None, HelpPhase::Exploration);

        assert!(lines
            .iter()
            .any(|line| line.contains("Commands overview (exploration)")));
        assert!(lines.iter().any(|line| line.contains("movement")));
        assert!(lines.iter().any(|line| line.contains("combat")));
    }

    #[test]
    fn help_topic_is_phase_aware() {
        let exploration_lines = render_help(Some("movement"), HelpPhase::Exploration);
        let combat_lines = render_help(Some("movement"), HelpPhase::Combat);

        assert!(exploration_lines
            .iter()
            .any(|line| line.contains("go <direction>")));
        assert!(combat_lines
            .iter()
            .any(|line| line.contains("approach <target>")));
    }

    #[test]
    fn help_spells_topic_is_class_agnostic() {
        let lines = render_help(Some("spells"), HelpPhase::Exploration);
        let joined = lines.join("\n");

        // Should not mention Wizard-only language.
        assert!(
            !joined.to_lowercase().contains("only wizards"),
            "Help text must not say 'Only Wizards'. Got:\n{}",
            joined,
        );
        // Should not hardcode INT mod as the sole spellcasting ability.
        assert!(
            !joined.contains("INT mod + proficiency vs AC"),
            "Help text must not hardcode 'INT mod + proficiency vs AC'. Got:\n{}",
            joined,
        );
        // Should mention that the ability varies by class.
        assert!(
            joined.to_lowercase().contains("spellcasting ability"),
            "Help text should mention a generic 'spellcasting ability'. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_unknown_topic_lists_valid_topics_for_phase() {
        let lines = render_help(Some("mystery"), HelpPhase::Combat);

        assert!(lines.iter().any(|line| line.contains("Unknown help topic")));
        assert!(lines
            .iter()
            .any(|line| line.contains("Valid topics during combat")));
        assert!(lines
            .iter()
            .any(|line| line
                .contains("movement, actions, inventory, equipment, spells, system, combat")));
    }

    #[test]
    fn help_combat_mentions_wait_and_pass() {
        let lines = render_help(Some("combat"), HelpPhase::Combat);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("wait"),
            "Combat help should mention 'wait'. Got:\n{}",
            joined,
        );
        assert!(
            joined.contains("pass"),
            "Combat help should mention 'pass'. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_actions_mentions_action_economy_and_barbarian_cues() {
        let lines = render_help(Some("actions"), HelpPhase::Combat);
        let joined = lines.join("\n").to_lowercase();

        assert!(joined.contains("bonus action"));
        assert!(joined.contains("reaction"));
        assert!(joined.contains("rage"));
        assert!(joined.contains("grapple"));
    }

    // ---- CharacterCreation help tests ----

    #[test]
    fn help_overview_lists_topics_for_creation() {
        let lines = render_help(None, HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("character creation"),
            "Expected overview to mention character creation. Got:\n{}",
            joined,
        );
        assert!(joined.contains("race"), "Expected overview to mention 'race'");
        assert!(
            joined.contains("class"),
            "Expected overview to mention 'class'"
        );
        assert!(
            joined.contains("abilities"),
            "Expected overview to mention 'abilities'"
        );
        assert!(
            joined.contains("checks"),
            "Expected overview to mention 'checks' (skill proficiency topic)"
        );
    }

    #[test]
    fn help_creation_race_topic() {
        let lines = render_help(Some("race"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("race"),
            "Expected race topic to mention races. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_creation_class_topic() {
        let lines = render_help(Some("class"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("class"),
            "Expected class topic to mention classes. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_creation_abilities_topic() {
        let lines = render_help(Some("abilities"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("ability"),
            "Expected abilities topic to mention ability scores. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_creation_skills_topic() {
        // "skills" normalizes to "checks" which is valid during creation
        let lines = render_help(Some("skills"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("skill"),
            "Expected skills topic to mention skill proficiencies. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_creation_unknown_topic() {
        let lines = render_help(Some("combat"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("not available during character creation"),
            "Expected unavailable topic message. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn help_creation_background_topic() {
        let lines = render_help(Some("background"), HelpPhase::CharacterCreation);
        let joined = lines.join("\n").to_lowercase();

        assert!(
            joined.contains("background"),
            "Expected background topic to mention backgrounds. Got:\n{}",
            joined,
        );
    }

    #[test]
    fn bark_templates_exist_for_all_roles() {
        assert!(BARK_GUARD.len() >= 3, "Expected at least 3 guard bark templates");
        assert!(BARK_MERCHANT.len() >= 3, "Expected at least 3 merchant bark templates");
        assert!(BARK_HERMIT.len() >= 3, "Expected at least 3 hermit bark templates");
        assert!(BARK_ADVENTURER.len() >= 3, "Expected at least 3 adventurer bark templates");
    }

    #[test]
    fn bark_templates_are_quoted_strings() {
        for bark in BARK_GUARD.iter().chain(BARK_MERCHANT.iter()).chain(BARK_HERMIT.iter()).chain(BARK_ADVENTURER.iter()) {
            assert!(bark.starts_with('"') && bark.ends_with('"'), "Bark template should be wrapped in quotes: {}", bark);
        }
    }

    // ---- Environment interaction template tests ----

    #[test]
    fn test_door_open_template_has_placeholder() {
        assert!(super::DOOR_OPEN.contains("{name}"));
    }

    #[test]
    fn test_door_templates_produce_correct_text() {
        let text = super::DOOR_OPEN.replace("{name}", "iron door");
        assert_eq!(text, "You push open the iron door.");
        let text = super::DOOR_LOCKED.replace("{name}", "iron door");
        assert_eq!(text, "The iron door is locked.");
        let text = super::DOOR_CLOSE.replace("{name}", "iron door");
        assert_eq!(text, "You pull the iron door closed.");
    }

    #[test]
    fn test_chest_templates_produce_correct_text() {
        let text = super::CHEST_OPEN.replace("{name}", "old chest");
        assert_eq!(text, "You lift the lid of the old chest.");
    }

    #[test]
    fn test_lever_toggle_template() {
        let text = super::LEVER_TOGGLE.replace("{verb}", "pull").replace("{name}", "lever");
        assert!(text.contains("pull") && text.contains("lever") && text.contains("clicks"));
    }

    #[test]
    fn test_climb_templates() {
        let text = super::CLIMB_SUCCESS.replace("{name}", "chains").replace("{check}", "[DC 10]");
        assert!(text.contains("climb the chains"));
        let text = super::CLIMB_FAIL.replace("{name}", "chains").replace("{check}", "[DC 10]");
        assert!(text.contains("lose your grip"));
    }
}