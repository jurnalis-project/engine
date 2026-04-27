// jurnalis-engine/src/parser/mod.rs
pub mod resolver;

use crate::types::{Direction, Skill};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Look(Option<String>),
    Search(Option<String>),
    Go(Direction),
    Talk(String),
    Take(String),
    TakeAll,
    Drop(String),
    Use(String),
    Equip(String),
    Unequip(String),
    Inventory,
    CharacterSheet,
    Check(String),
    Save(Option<String>),
    Load(Option<String>),
    Help(Option<String>),
    EndTurn,
    // Meta commands
    NewGame,
    Objective,
    Map,
    // Spell commands
    Spells,
    /// Cast a spell. `ritual == true` when the player added the `ritual` /
    /// `as ritual` suffix; the orchestrator validates the spell actually has
    /// the Ritual tag and skips slot consumption on the ritual path.
    Cast { spell: String, target: Option<String>, ritual: bool },
    // Combat commands
    Attack(String),
    Approach(String),
    Retreat,
    Dodge,
    Disengage,
    Dash,
    // Action-economy commands
    /// Off-hand attack (Two-Weapon Fighting). Consumes a bonus action.
    OffHandAttack(String),
    /// Dash as a bonus action (for Rogue Cunning Action or generic MVP).
    BonusDash,
    /// Disengage as a bonus action (Rogue Cunning Action). Consumes the bonus
    /// action, sets `player_disengaging = true`. Non-Rogues are rejected.
    BonusDisengage,
    /// Accept a pending reaction prompt. Only meaningful when
    /// `CombatState::pending_reaction` is Some; otherwise the orchestrator
    /// treats it as an unknown verb.
    ReactionYes,
    /// Decline a pending reaction prompt. Only meaningful when
    /// `CombatState::pending_reaction` is Some.
    ReactionNo,
    // Grappling commands
    /// Attempt to grapple an NPC. Argument is a free-form target name.
    Grapple(String),
    /// Attempt to escape from a grapple. No argument; orchestrator looks for
    /// an active Grappled condition on the player.
    EscapeGrapple,
    // Shove commands (2024 SRD unarmed strike option)
    /// Shove a target 5 feet away (push, no prone). Argument is a free-form target name.
    Shove(String),
    /// Shove a target and knock them prone. Argument is a free-form target name.
    ShoveProne(String),
    // Rest commands
    ShortRest,
    LongRest,
    // Class-feature commands
    /// Barbarian: enter Rage. No argument; orchestrator validates class & uses.
    Rage,
    /// Bard: grant Bardic Inspiration to a target ally (free-form name).
    BardicInspiration(String),
    /// Cleric / Paladin: invoke Channel Divinity. Subclass-specific effect is
    /// not selected in MVP; this just decrements the resource counter.
    ChannelDivinity,
    /// Paladin: spend points from the Lay on Hands pool. Bare form heals self;
    /// targeted form aims at an ally (free-form name).
    LayOnHands(String),
    /// Fighter: use Second Wind (bonus action, 1d10 + fighter level healing,
    /// once per short/long rest). No argument.
    SecondWind,
    /// Monk: spend a Ki / Focus point. The argument names the ability invoked
    /// (e.g. "flurry", "patient defense"). Treated as free-form for MVP.
    Ki(String),
    // Magic item commands (feat/magic-items, 2026-04-15).
    /// Attune to a magic item in inventory. Argument is the free-form item name.
    /// Orchestrator resolves it via fuzzy matching.
    Attune(String),
    /// Release attunement on an item. Argument is the free-form item name.
    Unattune(String),
    /// List the character's currently attuned items.
    ListAttunements,
    /// Take cover: grants the player Half cover until their next turn (or until
    /// they leave cover). Usable only in combat. See `docs/specs/cover-rules.md`.
    TakeCover,
    /// Take full cover: grants Three-Quarters cover (+5 AC, +5 DEX saves).
    /// Requires heavy cover features in the room. Falls back to Half if
    /// unavailable. Costs an action. See `docs/specs/cover-rules.md`.
    TakeFullCover,
    /// Leave cover: resets player cover to None. Free action (no action cost).
    /// See `docs/specs/cover-rules.md`.
    LeaveCover,
    /// Use a tool on a target. Parsed from "use <tool> on <target>".
    /// Orchestrator checks inventory, proficiency, and makes an ability check.
    UseTool { tool: String, target: String },
    /// Drink a consumable item (potion). In combat, costs a Bonus Action per
    /// 2024 SRD. Parsed from "drink <item>" or "quaff <item>".
    Drink(String),
    // --- Scenery interaction verbs ---
    /// Open a room feature (door, chest, etc.). Parsed from "open <target>".
    Open(String),
    /// Close a room feature. Parsed from "close <target>" or "shut <target>".
    Close(String),
    /// Push a room feature. Parsed from "push <target>".
    Push(String),
    /// Pull a room feature. Parsed from "pull <target>".
    Pull(String),
    /// Press a room feature (button, lever, etc.). Parsed from "press <target>".
    Press(String),
    /// Unlock a room feature. Parsed from "unlock <target>".
    Unlock(String),
    /// Force open a room feature. Parsed from "force <target>" or "break down <target>".
    Force(String),
    /// Climb a room feature. Parsed from "climb <target>".
    Climb(String),
    // --- Trade commands ---
    /// Browse a merchant's available wares and prices. No argument.
    Browse,
    /// Buy an item from a merchant NPC. Argument is the item name.
    Buy(String),
    /// Sell an item to a merchant NPC. Argument is the item name from inventory.
    Sell(String),
    /// Explicit ranged attack with an AMMUNITION weapon (bow, crossbow).
    /// Forces ranged mode; blocked if the equipped weapon lacks AMMUNITION.
    Shoot(String),
    /// Explicit thrown attack. Forces ranged mode for THROWN weapons even
    /// within melee range (overriding the default melee-at-5ft behavior).
    Throw(String),
    /// Fighter: Action Surge (level 2). Grants one additional action this
    /// turn. Parsed from "action surge" or "surge".
    ActionSurge,
    /// Barbarian: Reckless Attack (level 2). Declare a reckless attack on
    /// a target. Gives advantage on STR-based attack rolls until the start
    /// of the player's next turn; attack rolls against the player also have
    /// advantage. Parsed from "reckless attack <target>" / "recklessly
    /// attack <target>".
    RecklessAttack(String),
    /// Display active buffs, conditions, and effects on the player character.
    /// Parsed from "buffs", "conditions", "effects", or "active effects".
    Buffs,
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let input = input.trim();
    if input.is_empty() {
        return Command::Unknown(String::new());
    }

    let lower = input.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // 3-word phrases first (before 2-word phrases, to avoid mis-matches).
    if words.len() >= 3 {
        let three = format!("{} {} {}", words[0], words[1], words[2]);
        let rest_after_three: String = if words.len() > 3 { words[3..].join(" ") } else { String::new() };
        match three.as_str() {
            "take full cover" => return Command::TakeFullCover,
            "off hand attack" => {
                return if rest_after_three.is_empty() {
                    Command::Unknown("Off-hand attack what?".to_string())
                } else {
                    Command::OffHandAttack(rest_after_three)
                };
            }
            "dash as bonus" => return Command::BonusDash,
            "disengage as bonus" => return Command::BonusDisengage,
            // Fighter: "use second wind"
            "use second wind" => return Command::SecondWind,
            // Paladin: "lay on hands [target]"
            "lay on hands" => return Command::LayOnHands(rest_after_three),
            _ => {}
        }
    }

    // 2-word phrases first
    if words.len() >= 2 {
        let two = format!("{} {}", words[0], words[1]);
        let rest = if words.len() > 2 { words[2..].join(" ") } else { String::new() };

        match two.as_str() {
            "take cover" => return Command::TakeCover,
            "leave cover" => return Command::LeaveCover,
            "look at" | "check out" => {
                return if rest.is_empty() { Command::Look(None) } else { Command::Look(Some(rest)) };
            }
            "search for" => {
                return if rest.is_empty() { Command::Search(None) } else { Command::Search(Some(rest)) };
            }
            "look around" => return Command::Look(None),
            "list wares" | "look wares" => return Command::Browse,
            "go to" => {
                return parse_direction(&rest).map(Command::Go)
                    .unwrap_or(Command::Unknown(input.to_string()));
            }
            "talk to" | "speak to" | "speak with" => {
                return if rest.is_empty() {
                    Command::Unknown("Talk to whom?".to_string())
                } else {
                    Command::Talk(rest)
                };
            }
            "pick up" => {
                return if rest.is_empty() {
                    Command::Unknown("Pick up what?".to_string())
                } else {
                    Command::Take(rest)
                };
            }
            "put down" => {
                return if rest.is_empty() {
                    Command::Unknown("Drop what?".to_string())
                } else {
                    Command::Drop(rest)
                };
            }
            "put on" => {
                return if rest.is_empty() {
                    Command::Unknown("Equip what?".to_string())
                } else {
                    Command::Equip(rest)
                };
            }
            "take off" => {
                return if rest.is_empty() {
                    Command::Unknown("Unequip what?".to_string())
                } else {
                    Command::Unequip(rest)
                };
            }
            "swing at" => {
                return if rest.is_empty() {
                    Command::Unknown("Attack what?".to_string())
                } else {
                    Command::Attack(rest)
                };
            }
            "offhand attack" => {
                return if rest.is_empty() {
                    Command::Unknown("Off-hand attack what?".to_string())
                } else {
                    Command::OffHandAttack(rest)
                };
            }
            "bonus dash" | "dash bonus" => {
                return Command::BonusDash;
            }
            "bonus disengage" | "disengage bonus" | "cunning disengage" => {
                return Command::BonusDisengage;
            }
            "move to" | "move toward" => {
                // Check if it looks like a direction first
                if let Some(dir) = parse_direction(&rest) {
                    return Command::Go(dir);
                }
                return if rest.is_empty() {
                    Command::Unknown("Move toward what?".to_string())
                } else {
                    Command::Approach(rest)
                };
            }
            "move away" | "fall back" | "back up" => {
                return Command::Retreat;
            }
            "end turn" => {
                return Command::EndTurn;
            }
            "escape grapple" | "break grapple" | "break free" => {
                return Command::EscapeGrapple;
            }
            // Shove prone (2024 SRD): "shove prone <target>" / "push prone <target>"
            "shove prone" | "push prone" => {
                return if rest.is_empty() {
                    Command::Unknown("Shove prone whom?".to_string())
                } else {
                    Command::ShoveProne(rest)
                };
            }
            "spell list" | "known spells" | "my spells" => {
                return Command::Spells;
            }
            "active effects" | "active buffs" | "active conditions" => {
                return Command::Buffs;
            }
            "new game" => {
                return Command::NewGame;
            }
            "short rest" | "short sleep" | "short nap" | "short camp" => {
                return Command::ShortRest;
            }
            "long rest" | "long sleep" | "long nap" | "long camp" => {
                return Command::LongRest;
            }
            // Reversed order aliases: "sleep short", "camp long", etc.
            "sleep short" | "camp short" | "nap short" => {
                return Command::ShortRest;
            }
            "sleep long" | "camp long" | "nap long" => {
                return Command::LongRest;
            }
            // Barbarian: "enter rage"
            "enter rage" => return Command::Rage,
            // Fighter: "second wind"
            "second wind" => return Command::SecondWind,
            // Cleric / Paladin: "channel divinity"
            "channel divinity" => return Command::ChannelDivinity,
            // Bard: "bardic inspiration <target>"
            "bardic inspiration" => {
                return if rest.is_empty() {
                    Command::Unknown("Inspire whom?".to_string())
                } else {
                    Command::BardicInspiration(rest)
                };
            }
            // Magic items: "bond with <item>" attunes.
            "bond with" => {
                return if rest.is_empty() {
                    Command::Unknown("Attune to what?".to_string())
                } else {
                    Command::Attune(rest)
                };
            }
            // Fighter: "action surge"
            "action surge" => return Command::ActionSurge,
            // Barbarian: "reckless attack <target>" / "recklessly attack <target>"
            "reckless attack" | "recklessly attack" => {
                return if rest.is_empty() {
                    Command::Unknown("Reckless attack what?".to_string())
                } else {
                    Command::RecklessAttack(rest)
                };
            }
            // Scenery interaction: "break down <target>" -> Force
            "break down" => {
                return if rest.is_empty() {
                    Command::Unknown("Force open what?".to_string())
                } else {
                    Command::Force(rest)
                };
            }
            _ => {}
        }
    }

    // 1-word verbs
    let verb = words[0];
    let args = if words.len() > 1 { words[1..].join(" ") } else { String::new() };

    match verb {
        "look" | "l" | "examine" | "inspect" | "see" => {
            if args.is_empty() { Command::Look(None) } else { Command::Look(Some(args)) }
        }
        "search" => {
            if args.is_empty() { Command::Search(None) } else { Command::Search(Some(args)) }
        }
        "where" | "surroundings" => Command::Look(None),
        "go" | "walk" | "move" | "head" => {
            parse_direction(&args).map(Command::Go)
                .unwrap_or_else(|| if args.is_empty() {
                    Command::Unknown("Go where?".to_string())
                } else {
                    Command::Unknown(input.to_string())
                })
        }
        "n" | "north" => Command::Go(Direction::North),
        "s" | "south" => Command::Go(Direction::South),
        "e" | "east" => Command::Go(Direction::East),
        "w" | "west" => Command::Go(Direction::West),
        "u" | "up" => Command::Go(Direction::Up),
        "d" | "down" => Command::Go(Direction::Down),
        "talk" | "speak" | "ask" => {
            if args.is_empty() { Command::Unknown("Talk to whom?".to_string()) } else { Command::Talk(args) }
        }
        "take" | "get" | "grab" | "collect" => {
            if args.is_empty() {
                Command::Unknown("Take what?".to_string())
            } else if args == "all" || args == "everything" {
                Command::TakeAll
            } else {
                Command::Take(args)
            }
        }
        "drop" | "discard" => {
            if args.is_empty() { Command::Unknown("Drop what?".to_string()) } else { Command::Drop(args) }
        }
        "use" | "activate" | "apply" => {
            if args.is_empty() {
                Command::Unknown("Use what?".to_string())
            } else if let Some(pos) = args.find(" on ") {
                // "use <tool> on <target>"
                let tool = args[..pos].trim().to_string();
                let target = args[pos + 4..].trim().to_string();
                if tool.is_empty() || target.is_empty() {
                    Command::Use(args)
                } else {
                    Command::UseTool { tool, target }
                }
            } else {
                Command::Use(args)
            }
        }
        "equip" | "wear" | "wield" | "don" => {
            if args.is_empty() { Command::Unknown("Equip what?".to_string()) } else { Command::Equip(args) }
        }
        "unequip" | "doff" => {
            if args.is_empty() { Command::Unknown("Unequip what?".to_string()) } else { Command::Unequip(args) }
        }
        "spells" => Command::Spells,
        "buffs" | "conditions" | "effects" => Command::Buffs,
        "cast" => {
            if args.is_empty() {
                Command::Unknown("Cast what spell?".to_string())
            } else {
                // Detect a ritual-cast suffix. Accept "as ritual" or a bare
                // trailing " ritual". Strip it before further parsing so the
                // spell name doesn't include "ritual".
                let (args_no_ritual, ritual) = strip_ritual_suffix(&args);
                // Split on " at " or " on " to separate spell name from target
                let (spell, target) = if let Some(pos) = args_no_ritual.find(" at ") {
                    (args_no_ritual[..pos].to_string(),
                     Some(args_no_ritual[pos + 4..].to_string()))
                } else if let Some(pos) = args_no_ritual.find(" on ") {
                    (args_no_ritual[..pos].to_string(),
                     Some(args_no_ritual[pos + 4..].to_string()))
                } else {
                    (args_no_ritual.clone(), None)
                };
                let target = target.filter(|t| !t.is_empty());
                Command::Cast { spell, target, ritual }
            }
        }
        "attack" | "hit" | "strike" => {
            if args.is_empty() {
                Command::Unknown("Attack what?".to_string())
            } else if let Some(target) = strip_offhand_suffix(&args) {
                if target.is_empty() {
                    Command::Unknown("Off-hand attack what?".to_string())
                } else {
                    Command::OffHandAttack(target)
                }
            } else {
                Command::Attack(args)
            }
        }
        "shoot" | "fire" => {
            if args.is_empty() {
                Command::Unknown("Shoot what?".to_string())
            } else {
                Command::Shoot(args)
            }
        }
        "throw" | "hurl" | "toss" | "lob" => {
            if args.is_empty() {
                Command::Unknown("Throw what?".to_string())
            } else {
                Command::Throw(args)
            }
        }
        "grapple" | "wrestle" | "seize" => {
            if args.is_empty() {
                Command::Unknown("Grapple whom?".to_string())
            } else {
                Command::Grapple(args)
            }
        }
        "shove" => {
            if args.is_empty() {
                Command::Unknown("Shove whom?".to_string())
            } else {
                Command::Shove(args)
            }
        }
        "push" => {
            // "push" routes to Push (scenery interaction) in exploration,
            // not Shove. Use "shove" explicitly for combat.
            if args.is_empty() {
                Command::Unknown("Push what?".to_string())
            } else {
                Command::Push(args)
            }
        }
        "escape" => Command::EscapeGrapple,
        "approach" | "advance" | "close" => {
            if args.is_empty() { Command::Unknown("Approach what?".to_string()) } else { Command::Approach(args) }
        }
        "retreat" => Command::Retreat,
        "dodge" => Command::Dodge,
        "disengage" | "withdraw" | "flee" => Command::Disengage,
        "dash" | "run" | "sprint" => Command::Dash,
        "yes" | "y" => Command::ReactionYes,
        "no" => Command::ReactionNo,
        "end" | "pass" | "wait" => Command::EndTurn,
        "inventory" | "i" | "inv" | "items" | "bag" => Command::Inventory,
        "character" | "char" | "sheet" | "stats" | "status" => Command::CharacterSheet,
        "check" | "roll" | "try" => {
            if args.is_empty() { Command::Unknown("Check which skill?".to_string()) } else { Command::Check(args) }
        }
        "save" => { if args.is_empty() { Command::Save(None) } else { Command::Save(Some(args)) } }
        "load" | "restore" => { if args.is_empty() { Command::Load(None) } else { Command::Load(Some(args)) } }
        "help" | "?" | "commands" => { if args.is_empty() { Command::Help(None) } else { Command::Help(Some(args)) } }
        "newgame" | "restart" => Command::NewGame,
        "rest" | "sleep" | "camp" | "nap" => Command::Unknown("Short rest or long rest? Try 'short rest' or 'long rest'.".to_string()),
        "objective" | "goal" | "quest" => Command::Objective,
        "map" => Command::Map,
        // Fighter: "surge" -> Action Surge
        "surge" => Command::ActionSurge,
        // Barbarian: "rage"
        "rage" => Command::Rage,
        // Bard: "inspire <target>"
        "inspire" => {
            if args.is_empty() {
                Command::Unknown("Inspire whom?".to_string())
            } else {
                Command::BardicInspiration(args)
            }
        }
        // Fighter: "wind" (short alias for "second wind")
        "wind" => Command::SecondWind,
        // Monk: "ki <ability>"
        "ki" | "focus" => {
            if args.is_empty() {
                Command::Unknown("Spend ki on what? (e.g. 'ki flurry', 'ki patient defense')".to_string())
            } else {
                Command::Ki(args)
            }
        }
        // Magic items: attune / unattune / list attunements.
        "attune" => {
            if args.is_empty() {
                Command::Unknown("Attune to what? (e.g. 'attune cloak')".to_string())
            } else {
                Command::Attune(args)
            }
        }
        "drink" | "quaff" | "swallow" => {
            if args.is_empty() {
                Command::Unknown("Drink what?".to_string())
            } else {
                Command::Drink(args)
            }
        }
        "unattune" => {
            if args.is_empty() {
                Command::Unknown("Unattune what?".to_string())
            } else {
                Command::Unattune(args)
            }
        }
        "release" => {
            // `release` is a narrow alias for unattune; requires an argument
            // so we don't clash with generic "release" gestures.
            if args.is_empty() {
                Command::Unknown("Release what?".to_string())
            } else {
                Command::Unattune(args)
            }
        }
        "attunement" | "attunements" => Command::ListAttunements,
        // ---- Scenery interaction verbs ----
        "open" => {
            if args.is_empty() {
                Command::Unknown("Open what?".to_string())
            } else {
                Command::Open(args)
            }
        }
        // Note: "close" parses to Approach (combat: close the distance). Use
        // "shut" for closing doors/containers (scenery interaction).
        "shut" => {
            if args.is_empty() {
                Command::Unknown("Close what?".to_string())
            } else {
                Command::Close(args)
            }
        }
        "pull" => {
            if args.is_empty() {
                Command::Unknown("Pull what?".to_string())
            } else {
                Command::Pull(args)
            }
        }
        "press" => {
            if args.is_empty() {
                Command::Unknown("Press what?".to_string())
            } else {
                Command::Press(args)
            }
        }
        "unlock" => {
            if args.is_empty() {
                Command::Unknown("Unlock what?".to_string())
            } else {
                Command::Unlock(args)
            }
        }
        "force" | "break" => {
            if args.is_empty() {
                Command::Unknown("Force open what?".to_string())
            } else {
                Command::Force(args)
            }
        }
        "climb" | "scale" | "clamber" => {
            if args.is_empty() {
                Command::Unknown("Climb what?".to_string())
            } else {
                Command::Climb(args)
            }
        }
        // ---- Trade commands ----
        "browse" | "shop" | "wares" | "trade" => Command::Browse,
        "buy" | "purchase" => {
            if args.is_empty() {
                Command::Unknown("Buy what?".to_string())
            } else {
                Command::Buy(args)
            }
        }
        "sell" => {
            if args.is_empty() {
                Command::Unknown("Sell what?".to_string())
            } else {
                Command::Sell(args)
            }
        }
        _ => Command::Unknown(input.to_string()),
    }
}

/// Detect and strip a ritual-cast suffix from the arguments of a `cast`
/// command. Recognizes a bare trailing `ritual` or the phrase `as ritual`.
/// Returns the stripped argument string and a `ritual` flag.
///
/// Examples:
///   "detect magic ritual"          -> ("detect magic", true)
///   "detect magic as ritual"       -> ("detect magic", true)
///   "identify at chest"            -> ("identify at chest", false)
///   "detect magic as ritual" (mid) -> not stripped unless at end
fn strip_ritual_suffix(args: &str) -> (String, bool) {
    let trimmed = args.trim();
    for suffix in [" as ritual", " ritual"] {
        if let Some(stripped) = trimmed.strip_suffix(suffix) {
            return (stripped.trim().to_string(), true);
        }
    }
    (trimmed.to_string(), false)
}

/// If the argument string ends with an "off hand" / "offhand" suffix,
/// strip it and return the inner target. Returns None if the suffix
/// is not present.
fn strip_offhand_suffix(args: &str) -> Option<String> {
    let trimmed = args.trim();
    for suffix in &[" off hand", " offhand"] {
        if let Some(stripped) = trimmed.strip_suffix(suffix) {
            return Some(stripped.trim().to_string());
        }
    }
    // Also accept bare "offhand" / "off hand" with no target.
    if trimmed == "off hand" || trimmed == "offhand" {
        return Some(String::new());
    }
    None
}

fn parse_direction(s: &str) -> Option<Direction> {
    match s {
        "north" | "n" => Some(Direction::North),
        "south" | "s" => Some(Direction::South),
        "east" | "e" => Some(Direction::East),
        "west" | "w" => Some(Direction::West),
        "up" | "u" => Some(Direction::Up),
        "down" | "d" => Some(Direction::Down),
        _ => None,
    }
}

pub fn resolve_skill(name: &str) -> Option<Skill> {
    match name.to_lowercase().as_str() {
        "athletics" => Some(Skill::Athletics),
        "acrobatics" => Some(Skill::Acrobatics),
        "sleight of hand" | "sleight" => Some(Skill::SleightOfHand),
        "stealth" => Some(Skill::Stealth),
        "arcana" => Some(Skill::Arcana),
        "history" => Some(Skill::History),
        "investigation" => Some(Skill::Investigation),
        "nature" => Some(Skill::Nature),
        "religion" => Some(Skill::Religion),
        "animal handling" | "animal" => Some(Skill::AnimalHandling),
        "insight" => Some(Skill::Insight),
        "medicine" => Some(Skill::Medicine),
        "perception" => Some(Skill::Perception),
        "survival" => Some(Skill::Survival),
        "deception" => Some(Skill::Deception),
        "intimidation" => Some(Skill::Intimidation),
        "performance" => Some(Skill::Performance),
        "persuasion" => Some(Skill::Persuasion),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_look_no_target() {
        assert_eq!(parse("look"), Command::Look(None));
        assert_eq!(parse("l"), Command::Look(None));
    }

    #[test]
    fn test_look_with_target() {
        assert_eq!(parse("look chest"), Command::Look(Some("chest".to_string())));
        assert_eq!(parse("examine old door"), Command::Look(Some("old door".to_string())));
    }

    #[test]
    fn test_direction_shortcuts() {
        assert_eq!(parse("n"), Command::Go(Direction::North));
        assert_eq!(parse("s"), Command::Go(Direction::South));
        assert_eq!(parse("e"), Command::Go(Direction::East));
        assert_eq!(parse("w"), Command::Go(Direction::West));
        assert_eq!(parse("u"), Command::Go(Direction::Up));
        assert_eq!(parse("d"), Command::Go(Direction::Down));
    }

    #[test]
    fn test_go_direction() {
        assert_eq!(parse("go north"), Command::Go(Direction::North));
        assert_eq!(parse("go south"), Command::Go(Direction::South));
    }

    #[test]
    fn test_go_invalid() {
        match parse("go sideways") { Command::Unknown(_) => {} other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_talk() {
        assert_eq!(parse("talk merchant"), Command::Talk("merchant".to_string()));
        assert_eq!(parse("speak old man"), Command::Talk("old man".to_string()));
    }

    #[test]
    fn test_take_aliases() {
        assert_eq!(parse("take sword"), Command::Take("sword".to_string()));
        assert_eq!(parse("get key"), Command::Take("key".to_string()));
        assert_eq!(parse("pick up torch"), Command::Take("torch".to_string()));
    }

    #[test]
    fn test_inventory_aliases() {
        assert_eq!(parse("inventory"), Command::Inventory);
        assert_eq!(parse("i"), Command::Inventory);
        assert_eq!(parse("inv"), Command::Inventory);
    }

    #[test]
    fn test_character_sheet() {
        assert_eq!(parse("character"), Command::CharacterSheet);
        assert_eq!(parse("char"), Command::CharacterSheet);
        assert_eq!(parse("sheet"), Command::CharacterSheet);
    }

    #[test]
    fn test_save_load() {
        assert_eq!(parse("save"), Command::Save(None));
        assert_eq!(parse("save mysave"), Command::Save(Some("mysave".to_string())));
        assert_eq!(parse("load"), Command::Load(None));
        assert_eq!(parse("load mysave"), Command::Load(Some("mysave".to_string())));
    }

    #[test]
    fn test_help() {
        assert_eq!(parse("help"), Command::Help(None));
        assert_eq!(parse("?"), Command::Help(None));
        assert_eq!(parse("help look"), Command::Help(Some("look".to_string())));
    }

    #[test]
    fn test_case_insensitive() {
        assert_eq!(parse("LOOK"), Command::Look(None));
        assert_eq!(parse("Go North"), Command::Go(Direction::North));
    }

    #[test]
    fn test_empty_input() {
        match parse("") { Command::Unknown(_) => {} other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_unknown_command() {
        match parse("dance wildly") { Command::Unknown(s) => assert_eq!(s, "dance wildly"), other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_resolve_skill() {
        assert_eq!(resolve_skill("perception"), Some(Skill::Perception));
        assert_eq!(resolve_skill("Stealth"), Some(Skill::Stealth));
        assert_eq!(resolve_skill("sleight of hand"), Some(Skill::SleightOfHand));
        assert_eq!(resolve_skill("nonsense"), None);
    }

    #[test]
    fn test_verb_phrase_look_at() {
        assert_eq!(parse("look at chest"), Command::Look(Some("chest".to_string())));
    }

    #[test]
    fn test_verb_phrase_talk_to() {
        assert_eq!(parse("talk to magnus"), Command::Talk("magnus".to_string()));
    }

    #[test]
    fn test_verb_phrase_pick_up() {
        assert_eq!(parse("pick up torch"), Command::Take("torch".to_string()));
    }

    #[test]
    fn test_verb_phrase_speak_with() {
        assert_eq!(parse("speak with elder"), Command::Talk("elder".to_string()));
    }

    #[test]
    fn test_verb_phrase_check_out() {
        assert_eq!(parse("check out door"), Command::Look(Some("door".to_string())));
    }

    #[test]
    fn test_verb_phrase_put_down() {
        assert_eq!(parse("put down sword"), Command::Drop("sword".to_string()));
    }

    #[test]
    fn test_verb_phrase_go_to() {
        assert_eq!(parse("go to north"), Command::Go(Direction::North));
    }

    #[test]
    fn test_look_aliases() {
        assert_eq!(parse("examine chest"), Command::Look(Some("chest".to_string())));
        assert_eq!(parse("inspect door"), Command::Look(Some("door".to_string())));
        assert_eq!(parse("see sword"), Command::Look(Some("sword".to_string())));
    }

    #[test]
    fn test_search_variants() {
        assert_eq!(parse("search"), Command::Search(None));
        assert_eq!(parse("search room"), Command::Search(Some("room".to_string())));
        assert_eq!(parse("search for trap"), Command::Search(Some("trap".to_string())));
    }

    #[test]
    fn test_look_around_variants() {
        assert_eq!(parse("look"), Command::Look(None));
        assert_eq!(parse("look around"), Command::Look(None));
        assert_eq!(parse("where"), Command::Look(None));
        assert_eq!(parse("surroundings"), Command::Look(None));
    }

    #[test]
    fn test_go_aliases() {
        assert_eq!(parse("walk north"), Command::Go(Direction::North));
        assert_eq!(parse("move south"), Command::Go(Direction::South));
        assert_eq!(parse("head east"), Command::Go(Direction::East));
        assert_eq!(parse("go to west"), Command::Go(Direction::West));
    }

    #[test]
    fn test_talk_aliases() {
        assert_eq!(parse("speak elder"), Command::Talk("elder".to_string()));
        assert_eq!(parse("ask guard"), Command::Talk("guard".to_string()));
        assert_eq!(parse("talk to merchant"), Command::Talk("merchant".to_string()));
        assert_eq!(parse("speak to wizard"), Command::Talk("wizard".to_string()));
        assert_eq!(parse("speak with hermit"), Command::Talk("hermit".to_string()));
    }

    #[test]
    fn test_take_extra_aliases() {
        assert_eq!(parse("grab torch"), Command::Take("torch".to_string()));
        assert_eq!(parse("collect gems"), Command::Take("gems".to_string()));
    }

    #[test]
    fn test_drop_aliases() {
        assert_eq!(parse("drop sword"), Command::Drop("sword".to_string()));
        assert_eq!(parse("discard junk"), Command::Drop("junk".to_string()));
        assert_eq!(parse("put down shield"), Command::Drop("shield".to_string()));
    }

    #[test]
    fn test_use_aliases() {
        assert_eq!(parse("activate lever"), Command::Use("lever".to_string()));
        assert_eq!(parse("apply potion"), Command::Use("potion".to_string()));
    }

    #[test]
    fn test_inventory_extra_aliases() {
        assert_eq!(parse("items"), Command::Inventory);
        assert_eq!(parse("bag"), Command::Inventory);
    }

    #[test]
    fn test_character_aliases() {
        assert_eq!(parse("stats"), Command::CharacterSheet);
        assert_eq!(parse("status"), Command::CharacterSheet);
    }

    #[test]
    fn test_check_aliases() {
        assert_eq!(parse("roll perception"), Command::Check("perception".to_string()));
        assert_eq!(parse("try stealth"), Command::Check("stealth".to_string()));
    }

    #[test]
    fn test_load_alias() {
        assert_eq!(parse("restore"), Command::Load(None));
        assert_eq!(parse("restore mysave"), Command::Load(Some("mysave".to_string())));
    }

    #[test]
    fn test_help_aliases() {
        assert_eq!(parse("commands"), Command::Help(None));
    }

    #[test]
    fn test_attune_command() {
        assert_eq!(parse("attune cloak"), Command::Attune("cloak".to_string()));
        assert_eq!(parse("attune cloak of protection"), Command::Attune("cloak of protection".to_string()));
    }

    #[test]
    fn test_attune_bare_is_unknown_with_help() {
        match parse("attune") { Command::Unknown(s) => assert!(s.to_lowercase().contains("attune"), "got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_unattune_command() {
        assert_eq!(parse("unattune cloak"), Command::Unattune("cloak".to_string()));
    }

    #[test]
    fn test_unattune_bare_is_unknown_with_help() {
        match parse("unattune") { Command::Unknown(s) => assert!(s.to_lowercase().contains("unattune") || s.to_lowercase().contains("what"), "got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_list_attunements_command() {
        assert_eq!(parse("attunement"), Command::ListAttunements);
        assert_eq!(parse("attunements"), Command::ListAttunements);
    }

    #[test]
    fn test_attune_phrase_bond_with() {
        assert_eq!(parse("bond with ring"), Command::Attune("ring".to_string()));
    }

    #[test]
    fn test_unattune_alias_release() {
        assert_eq!(parse("release ring"), Command::Unattune("ring".to_string()));
    }

    #[test]
    fn test_bare_verbs_give_helpful_errors() {
        match parse("talk") { Command::Unknown(s) => assert!(s.contains("whom"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("take") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("drop") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("use") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("go") { Command::Unknown(s) => assert!(s.contains("where"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_multi_word_targets() {
        assert_eq!(parse("look at old rusty key"), Command::Look(Some("old rusty key".to_string())));
        assert_eq!(parse("talk to the wise elder"), Command::Talk("the wise elder".to_string()));
        assert_eq!(parse("pick up torn map"), Command::Take("torn map".to_string()));
    }

    #[test]
    fn test_check_out_no_target() {
        // "check out" with no target resolves to Look(None), not Check error
        assert_eq!(parse("check out"), Command::Look(None));
    }

    #[test]
    fn test_equip_command() {
        assert_eq!(parse("equip longsword"), Command::Equip("longsword".to_string()));
        assert_eq!(parse("wield dagger"), Command::Equip("dagger".to_string()));
        assert_eq!(parse("wear chain mail"), Command::Equip("chain mail".to_string()));
        assert_eq!(parse("don leather"), Command::Equip("leather".to_string()));
        assert_eq!(parse("put on chain mail"), Command::Equip("chain mail".to_string()));
    }

    #[test]
    fn test_unequip_command() {
        assert_eq!(parse("unequip longsword"), Command::Unequip("longsword".to_string()));
        assert_eq!(parse("take off chain mail"), Command::Unequip("chain mail".to_string()));
        assert_eq!(parse("doff plate"), Command::Unequip("plate".to_string()));
    }

    #[test]
    fn test_equip_bare_verb_error() {
        match parse("equip") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("unequip") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
    }

    // ---- Class-feature commands (feat/remaining-srd-classes) ----

    #[test]
    fn test_rage_command() {
        assert_eq!(parse("rage"), Command::Rage);
        assert_eq!(parse("RAGE"), Command::Rage);
        assert_eq!(parse("enter rage"), Command::Rage);
    }

    #[test]
    fn test_bardic_inspiration_command() {
        assert_eq!(parse("inspire ally"), Command::BardicInspiration("ally".to_string()));
        assert_eq!(
            parse("bardic inspiration friend"),
            Command::BardicInspiration("friend".to_string())
        );
    }

    #[test]
    fn test_bardic_inspiration_bare_verb_errors() {
        match parse("inspire") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("whom") || s.to_lowercase().contains("who")),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_channel_divinity_command() {
        assert_eq!(parse("channel divinity"), Command::ChannelDivinity);
        assert_eq!(parse("CHANNEL DIVINITY"), Command::ChannelDivinity);
    }

    #[test]
    fn test_lay_on_hands_command() {
        // Bare form targets self (empty target string).
        assert_eq!(parse("lay on hands"), Command::LayOnHands(String::new()));
        // With a target.
        assert_eq!(parse("lay on hands self"), Command::LayOnHands("self".to_string()));
    }

    #[test]
    fn test_ki_command() {
        assert_eq!(parse("ki flurry"), Command::Ki("flurry".to_string()));
        assert_eq!(parse("ki patient defense"), Command::Ki("patient defense".to_string()));
    }

    #[test]
    fn test_ki_bare_verb_errors() {
        match parse("ki") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("ki")),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ---- Second Wind (Fighter) ----

    #[test]
    fn test_second_wind_command() {
        assert_eq!(parse("second wind"), Command::SecondWind);
        assert_eq!(parse("Second Wind"), Command::SecondWind);
        assert_eq!(parse("SECOND WIND"), Command::SecondWind);
    }

    #[test]
    fn test_second_wind_use_phrase() {
        assert_eq!(parse("use second wind"), Command::SecondWind);
        assert_eq!(parse("Use Second Wind"), Command::SecondWind);
    }

    #[test]
    fn test_second_wind_alias_wind() {
        assert_eq!(parse("wind"), Command::SecondWind);
        assert_eq!(parse("Wind"), Command::SecondWind);
    }

    #[test]
    fn test_attack_command() {
        assert_eq!(parse("attack goblin"), Command::Attack("goblin".to_string()));
        assert_eq!(parse("hit orc"), Command::Attack("orc".to_string()));
        assert_eq!(parse("strike skeleton"), Command::Attack("skeleton".to_string()));
        assert_eq!(parse("swing at goblin"), Command::Attack("goblin".to_string()));
    }

    #[test]
    fn test_approach_command() {
        assert_eq!(parse("approach goblin"), Command::Approach("goblin".to_string()));
        assert_eq!(parse("advance goblin"), Command::Approach("goblin".to_string()));
        assert_eq!(parse("close goblin"), Command::Approach("goblin".to_string()));
        assert_eq!(parse("move to goblin"), Command::Approach("goblin".to_string()));
        assert_eq!(parse("move toward goblin"), Command::Approach("goblin".to_string()));
    }

    #[test]
    fn test_retreat_command() {
        assert_eq!(parse("retreat"), Command::Retreat);
        assert_eq!(parse("move away"), Command::Retreat);
        assert_eq!(parse("fall back"), Command::Retreat);
        assert_eq!(parse("back up"), Command::Retreat);
    }

    #[test]
    fn test_dodge_command() {
        assert_eq!(parse("dodge"), Command::Dodge);
    }

    #[test]
    fn test_disengage_command() {
        assert_eq!(parse("disengage"), Command::Disengage);
        assert_eq!(parse("withdraw"), Command::Disengage);
        assert_eq!(parse("flee"), Command::Disengage);
    }

    #[test]
    fn test_dash_command() {
        assert_eq!(parse("dash"), Command::Dash);
        assert_eq!(parse("run"), Command::Dash);
        assert_eq!(parse("sprint"), Command::Dash);
    }

    #[test]
    fn test_end_turn_command() {
        assert_eq!(parse("end turn"), Command::EndTurn);
        assert_eq!(parse("end"), Command::EndTurn);
        assert_eq!(parse("pass"), Command::EndTurn);
        assert_eq!(parse("wait"), Command::EndTurn);
    }

    #[test]
    fn test_combat_bare_verbs_give_helpful_errors() {
        match parse("attack") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
        match parse("approach") { Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s), other => panic!("Expected Unknown, got {:?}", other) }
    }

    #[test]
    fn test_move_to_direction_still_works() {
        // "move to north" should still resolve as Go(North), not Approach
        assert_eq!(parse("move to north"), Command::Go(Direction::North));
    }

    #[test]
    fn test_put_on_vs_put_down() {
        // "put on" -> Equip, "put down" -> Drop
        assert_eq!(parse("put on chain mail"), Command::Equip("chain mail".to_string()));
        assert_eq!(parse("put down sword"), Command::Drop("sword".to_string()));
    }

    #[test]
    fn test_spells_command() {
        assert_eq!(parse("spells"), Command::Spells);
    }

    #[test]
    fn test_spells_command_aliases() {
        assert_eq!(parse("spell list"), Command::Spells);
        assert_eq!(parse("known spells"), Command::Spells);
        assert_eq!(parse("my spells"), Command::Spells);
    }

    #[test]
    fn test_spells_command_case_insensitive() {
        assert_eq!(parse("SPELLS"), Command::Spells);
        assert_eq!(parse("Spell List"), Command::Spells);
        assert_eq!(parse("Known Spells"), Command::Spells);
    }

    #[test]
    fn test_cast_spell_at_target() {
        assert_eq!(
            parse("cast fire bolt at goblin"),
            Command::Cast { spell: "fire bolt".to_string(), target: Some("goblin".to_string()), ritual: false }
        );
    }

    #[test]
    fn test_cast_spell_no_target() {
        assert_eq!(
            parse("cast burning hands"),
            Command::Cast { spell: "burning hands".to_string(), target: None, ritual: false }
        );
    }

    #[test]
    fn test_cast_spell_on_target() {
        assert_eq!(
            parse("cast magic missile on skeleton"),
            Command::Cast { spell: "magic missile".to_string(), target: Some("skeleton".to_string()), ritual: false }
        );
    }

    #[test]
    fn test_cast_bare_verb_error() {
        match parse("cast") {
            Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_cast_prestidigitation() {
        assert_eq!(
            parse("cast prestidigitation"),
            Command::Cast { spell: "prestidigitation".to_string(), target: None, ritual: false }
        );
    }

    #[test]
    fn test_cast_shield() {
        assert_eq!(
            parse("cast shield"),
            Command::Cast { spell: "shield".to_string(), target: None, ritual: false }
        );
    }

    #[test]
    fn test_cast_sleep() {
        assert_eq!(
            parse("cast sleep"),
            Command::Cast { spell: "sleep".to_string(), target: None, ritual: false }
        );
    }

    // ---- Ritual casting (feat/expanded-spell-catalog) ----

    #[test]
    fn test_cast_ritual_suffix() {
        assert_eq!(
            parse("cast detect magic ritual"),
            Command::Cast { spell: "detect magic".to_string(), target: None, ritual: true }
        );
    }

    #[test]
    fn test_cast_as_ritual_phrase() {
        assert_eq!(
            parse("cast detect magic as ritual"),
            Command::Cast { spell: "detect magic".to_string(), target: None, ritual: true }
        );
    }

    #[test]
    fn test_cast_ritual_with_target() {
        assert_eq!(
            parse("cast identify on chest ritual"),
            Command::Cast { spell: "identify".to_string(), target: Some("chest".to_string()), ritual: true }
        );
    }

    #[test]
    fn test_cast_non_ritual_by_default() {
        // A spell whose name ends in "al" shouldn't be treated as ritual.
        // "cast identify" -> ritual false.
        assert_eq!(
            parse("cast identify"),
            Command::Cast { spell: "identify".to_string(), target: None, ritual: false }
        );
    }

    #[test]
    fn test_take_off_vs_take() {
        // "take off" -> Unequip, "take" -> Take
        assert_eq!(parse("take off helmet"), Command::Unequip("helmet".to_string()));
        assert_eq!(parse("take sword"), Command::Take("sword".to_string()));
    }

    // ---- Bulk pickup ----
    #[test]
    fn test_take_all_routes_to_bulk_pickup() {
        assert_eq!(parse("take all"), Command::TakeAll);
    }

    #[test]
    fn test_take_everything_routes_to_bulk_pickup() {
        assert_eq!(parse("take everything"), Command::TakeAll);
    }

    #[test]
    fn test_take_specific_item_still_works() {
        // Ensure "take" with a non-"all" argument still routes to Take
        assert_eq!(parse("take torch"), Command::Take("torch".to_string()));
    }

    #[test]
    fn test_objective_aliases() {
        assert_eq!(parse("objective"), Command::Objective);
        assert_eq!(parse("goal"), Command::Objective);
        assert_eq!(parse("quest"), Command::Objective);
    }

    #[test]
    fn test_map_aliases() {
        assert_eq!(parse("map"), Command::Map);
    }

    #[test]
    fn test_new_game_command() {
        assert_eq!(parse("new game"), Command::NewGame);
        assert_eq!(parse("newgame"), Command::NewGame);
        assert_eq!(parse("restart"), Command::NewGame);
        assert_eq!(parse("New Game"), Command::NewGame);
    }

    #[test]
    fn test_short_rest_command() {
        assert_eq!(parse("short rest"), Command::ShortRest);
        assert_eq!(parse("Short Rest"), Command::ShortRest);
        assert_eq!(parse("SHORT REST"), Command::ShortRest);
    }

    #[test]
    fn test_long_rest_command() {
        assert_eq!(parse("long rest"), Command::LongRest);
        assert_eq!(parse("Long Rest"), Command::LongRest);
        assert_eq!(parse("LONG REST"), Command::LongRest);
    }

    // ---- Action economy tests ----

    #[test]
    fn test_offhand_attack_variants() {
        assert_eq!(parse("offhand attack goblin"), Command::OffHandAttack("goblin".to_string()));
        assert_eq!(parse("off hand attack goblin"), Command::OffHandAttack("goblin".to_string()));
        assert_eq!(parse("attack goblin off hand"), Command::OffHandAttack("goblin".to_string()));
        assert_eq!(parse("attack goblin offhand"), Command::OffHandAttack("goblin".to_string()));
    }

    #[test]
    fn test_offhand_attack_multi_word_target() {
        assert_eq!(
            parse("attack giant rat off hand"),
            Command::OffHandAttack("giant rat".to_string())
        );
    }

    #[test]
    fn test_offhand_bare_verb_error() {
        match parse("offhand attack") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what")),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_bonus_dash_variants() {
        assert_eq!(parse("bonus dash"), Command::BonusDash);
        assert_eq!(parse("dash as bonus"), Command::BonusDash);
        assert_eq!(parse("dash bonus"), Command::BonusDash);
    }

    #[test]
    fn test_bonus_disengage_variants() {
        assert_eq!(parse("bonus disengage"), Command::BonusDisengage);
        assert_eq!(parse("disengage as bonus"), Command::BonusDisengage);
        assert_eq!(parse("disengage bonus"), Command::BonusDisengage);
        assert_eq!(parse("cunning disengage"), Command::BonusDisengage);
    }

    #[test]
    fn test_regular_disengage_still_parses_after_bonus_disengage() {
        // Ensure existing Disengage still works without bonus markers.
        assert_eq!(parse("disengage"), Command::Disengage);
        assert_eq!(parse("withdraw"), Command::Disengage);
        assert_eq!(parse("flee"), Command::Disengage);
    }

    #[test]
    fn test_regular_dash_still_parses() {
        // Ensure the existing Dash command still works without bonus markers.
        assert_eq!(parse("dash"), Command::Dash);
        assert_eq!(parse("run"), Command::Dash);
        assert_eq!(parse("sprint"), Command::Dash);
    }

    #[test]
    fn test_reaction_yes_commands() {
        assert_eq!(parse("yes"), Command::ReactionYes);
        assert_eq!(parse("y"), Command::ReactionYes);
    }

    #[test]
    fn test_reaction_no_commands() {
        assert_eq!(parse("no"), Command::ReactionNo);
        assert_eq!(parse("pass"), Command::EndTurn); // pass remains end-turn
    }

    #[test]
    fn test_attack_still_works_after_offhand_additions() {
        // Regression: plain `attack <target>` must still produce Attack, not OffHandAttack.
        assert_eq!(parse("attack goblin"), Command::Attack("goblin".to_string()));
    }

    #[test]
    fn test_bare_rest_disambiguates() {
        match parse("rest") {
            Command::Unknown(s) => {
                assert!(
                    s.to_lowercase().contains("short") && s.to_lowercase().contains("long"),
                    "Bare 'rest' should ask short vs long. Got: {}",
                    s,
                );
            }
            other => panic!("Expected Unknown for bare 'rest', got {:?}", other),
        }
    }

    // ---- Grappling commands ----

    #[test]
    fn test_grapple_command() {
        assert_eq!(parse("grapple goblin"), Command::Grapple("goblin".to_string()));
        assert_eq!(parse("wrestle orc"), Command::Grapple("orc".to_string()));
        assert_eq!(parse("seize bandit"), Command::Grapple("bandit".to_string()));
    }

    #[test]
    fn test_grapple_multi_word_target() {
        assert_eq!(parse("grapple giant rat"), Command::Grapple("giant rat".to_string()));
    }

    #[test]
    fn test_grapple_bare_verb_error() {
        match parse("grapple") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("whom")),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_escape_grapple_phrases() {
        assert_eq!(parse("escape grapple"), Command::EscapeGrapple);
        assert_eq!(parse("break grapple"), Command::EscapeGrapple);
        assert_eq!(parse("break free"), Command::EscapeGrapple);
    }

    #[test]
    fn test_escape_bare_verb() {
        assert_eq!(parse("escape"), Command::EscapeGrapple);
    }

    #[test]
    fn test_bare_attack_is_disambiguation_hint() {
        match parse("attack") {
            Command::Unknown(s) => assert!(s.contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_bare_rest_produces_hint_not_echoed_input() {
        match parse("rest") {
            Command::Unknown(s) => {
                assert_ne!(s, "rest", "bare 'rest' hint should NOT echo user input");
                assert!(
                    s.contains("short") || s.contains("long"),
                    "hint should mention short/long rest, got: {}",
                    s
                );
            }
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ---- Rest alias tests (sleep, camp, nap) ----

    #[test]
    fn test_sleep_alias_prompts_rest_type() {
        // Bare "sleep" should behave like bare "rest": ask short vs long.
        match parse("sleep") {
            Command::Unknown(s) => {
                assert!(
                    s.to_lowercase().contains("short") && s.to_lowercase().contains("long"),
                    "Bare 'sleep' should ask short vs long. Got: {}",
                    s,
                );
            }
            other => panic!("Expected Unknown for bare 'sleep', got {:?}", other),
        }
    }

    #[test]
    fn test_camp_alias_prompts_rest_type() {
        match parse("camp") {
            Command::Unknown(s) => {
                assert!(
                    s.to_lowercase().contains("short") && s.to_lowercase().contains("long"),
                    "Bare 'camp' should ask short vs long. Got: {}",
                    s,
                );
            }
            other => panic!("Expected Unknown for bare 'camp', got {:?}", other),
        }
    }

    #[test]
    fn test_nap_alias_prompts_rest_type() {
        match parse("nap") {
            Command::Unknown(s) => {
                assert!(
                    s.to_lowercase().contains("short") && s.to_lowercase().contains("long"),
                    "Bare 'nap' should ask short vs long. Got: {}",
                    s,
                );
            }
            other => panic!("Expected Unknown for bare 'nap', got {:?}", other),
        }
    }

    #[test]
    fn test_short_sleep_maps_to_short_rest() {
        assert_eq!(parse("short sleep"), Command::ShortRest);
        assert_eq!(parse("Short Sleep"), Command::ShortRest);
    }

    #[test]
    fn test_long_sleep_maps_to_long_rest() {
        assert_eq!(parse("long sleep"), Command::LongRest);
        assert_eq!(parse("Long Sleep"), Command::LongRest);
    }

    #[test]
    fn test_short_nap_maps_to_short_rest() {
        assert_eq!(parse("short nap"), Command::ShortRest);
    }

    #[test]
    fn test_long_nap_maps_to_long_rest() {
        assert_eq!(parse("long nap"), Command::LongRest);
    }

    #[test]
    fn test_short_camp_maps_to_short_rest() {
        assert_eq!(parse("short camp"), Command::ShortRest);
    }

    #[test]
    fn test_long_camp_maps_to_long_rest() {
        assert_eq!(parse("long camp"), Command::LongRest);
    }

    #[test]
    fn test_camp_short_maps_to_short_rest() {
        assert_eq!(parse("camp short"), Command::ShortRest);
    }

    #[test]
    fn test_camp_long_maps_to_long_rest() {
        assert_eq!(parse("camp long"), Command::LongRest);
    }

    #[test]
    fn test_sleep_short_maps_to_short_rest() {
        assert_eq!(parse("sleep short"), Command::ShortRest);
    }

    #[test]
    fn test_sleep_long_maps_to_long_rest() {
        assert_eq!(parse("sleep long"), Command::LongRest);
    }

    #[test]
    fn test_nap_short_maps_to_short_rest() {
        assert_eq!(parse("nap short"), Command::ShortRest);
    }

    #[test]
    fn test_nap_long_maps_to_long_rest() {
        assert_eq!(parse("nap long"), Command::LongRest);
    }

    // ---- Scenery interaction verbs ----

    #[test]
    fn test_open_command() {
        assert_eq!(parse("open door"), Command::Open("door".to_string()));
        assert_eq!(parse("open chest"), Command::Open("chest".to_string()));
        assert_eq!(parse("open rusty door"), Command::Open("rusty door".to_string()));
    }

    #[test]
    fn test_open_bare_verb_error() {
        match parse("open") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ---- Trade commands ----

    #[test]
    fn test_buy_command() {
        assert_eq!(parse("buy torch"), Command::Buy("torch".to_string()));
        assert_eq!(parse("buy longsword"), Command::Buy("longsword".to_string()));
        assert_eq!(parse("buy chain mail"), Command::Buy("chain mail".to_string()));
    }

    #[test]
    fn test_buy_alias_purchase() {
        assert_eq!(parse("purchase dagger"), Command::Buy("dagger".to_string()));
    }

    #[test]
    fn test_buy_bare_verb_error() {
        match parse("buy") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_shut_command() {
        assert_eq!(parse("shut door"), Command::Close("door".to_string()));
        assert_eq!(parse("shut chest"), Command::Close("chest".to_string()));
    }

    #[test]
    fn test_push_routes_to_scenery_not_shove() {
        assert_eq!(parse("push door"), Command::Push("door".to_string()));
        assert_eq!(parse("push lever"), Command::Push("lever".to_string()));
    }

    #[test]
    fn test_shove_still_works_for_combat() {
        assert_eq!(parse("shove goblin"), Command::Shove("goblin".to_string()));
    }

    #[test]
    fn test_pull_command() {
        assert_eq!(parse("pull chain"), Command::Pull("chain".to_string()));
        assert_eq!(parse("pull lever"), Command::Pull("lever".to_string()));
    }

    #[test]
    fn test_press_command() {
        assert_eq!(parse("press button"), Command::Press("button".to_string()));
        assert_eq!(parse("press rune"), Command::Press("rune".to_string()));
    }

    #[test]
    fn test_unlock_command() {
        assert_eq!(parse("unlock door"), Command::Unlock("door".to_string()));
        assert_eq!(parse("unlock chest"), Command::Unlock("chest".to_string()));
    }

    #[test]
    fn test_force_command() {
        assert_eq!(parse("force door"), Command::Force("door".to_string()));
        assert_eq!(parse("break door"), Command::Force("door".to_string()));
        assert_eq!(parse("break down door"), Command::Force("door".to_string()));
    }

    #[test]
    fn test_scenery_bare_verbs_give_errors() {
        match parse("pull") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown for bare 'pull', got {:?}", other),
        }
        match parse("press") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown for bare 'press', got {:?}", other),
        }
        match parse("unlock") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown for bare 'unlock', got {:?}", other),
        }
        match parse("force") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown for bare 'force', got {:?}", other),
        }
    }

    #[test]
    fn test_sell_command() {
        assert_eq!(parse("sell torch"), Command::Sell("torch".to_string()));
        assert_eq!(parse("sell longsword"), Command::Sell("longsword".to_string()));
    }

    #[test]
    fn test_sell_bare_verb_error() {
        match parse("sell") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    // ---- Browse command ----

    #[test]
    fn test_browse_command() {
        assert_eq!(parse("browse"), Command::Browse);
    }

    #[test]
    fn test_browse_alias_shop() {
        assert_eq!(parse("shop"), Command::Browse);
    }

    #[test]
    fn test_browse_alias_wares() {
        assert_eq!(parse("wares"), Command::Browse);
    }

    #[test]
    fn test_browse_phrase_list_wares() {
        assert_eq!(parse("list wares"), Command::Browse);
    }

    #[test]
    fn test_browse_phrase_look_wares() {
        assert_eq!(parse("look wares"), Command::Browse);
    }

    #[test]
    fn test_browse_alias_trade() {
        assert_eq!(parse("trade"), Command::Browse);
    }

    #[test]
    fn test_browse_case_insensitive() {
        assert_eq!(parse("BROWSE"), Command::Browse);
        assert_eq!(parse("Shop"), Command::Browse);
        assert_eq!(parse("Trade"), Command::Browse);
        assert_eq!(parse("List Wares"), Command::Browse);
    }

    #[test]
    fn test_buy_case_insensitive() {
        assert_eq!(parse("BUY TORCH"), Command::Buy("torch".to_string()));
        assert_eq!(parse("Purchase Dagger"), Command::Buy("dagger".to_string()));
    }

    // ---- Shoot command (ranged attack with AMMUNITION weapons) ----

    #[test]
    fn test_shoot_command() {
        assert_eq!(parse("shoot goblin"), Command::Shoot("goblin".to_string()));
        assert_eq!(parse("shoot orc"), Command::Shoot("orc".to_string()));
    }

    #[test]
    fn test_shoot_alias_fire() {
        assert_eq!(parse("fire goblin"), Command::Shoot("goblin".to_string()));
    }

    #[test]
    fn test_shoot_multi_word_target() {
        assert_eq!(parse("shoot giant rat"), Command::Shoot("giant rat".to_string()));
    }

    #[test]
    fn test_shoot_bare_verb_error() {
        match parse("shoot") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("shoot"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_shoot_case_insensitive() {
        assert_eq!(parse("SHOOT GOBLIN"), Command::Shoot("goblin".to_string()));
        assert_eq!(parse("Fire Orc"), Command::Shoot("orc".to_string()));
    }

    // ---- Throw command (explicit thrown attack) ----

    #[test]
    fn test_throw_command() {
        assert_eq!(parse("throw goblin"), Command::Throw("goblin".to_string()));
        assert_eq!(parse("throw orc"), Command::Throw("orc".to_string()));
    }

    #[test]
    fn test_throw_aliases() {
        assert_eq!(parse("hurl goblin"), Command::Throw("goblin".to_string()));
        assert_eq!(parse("toss goblin"), Command::Throw("goblin".to_string()));
        assert_eq!(parse("lob goblin"), Command::Throw("goblin".to_string()));
    }

    #[test]
    fn test_throw_multi_word_target() {
        assert_eq!(parse("throw giant rat"), Command::Throw("giant rat".to_string()));
    }

    #[test]
    fn test_throw_bare_verb_error() {
        match parse("throw") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("throw"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_throw_case_insensitive() {
        assert_eq!(parse("THROW GOBLIN"), Command::Throw("goblin".to_string()));
        assert_eq!(parse("Hurl Orc"), Command::Throw("orc".to_string()));
    }

    // ---- Cover commands ----

    #[test]
    fn test_leave_cover_command() {
        assert_eq!(parse("leave cover"), Command::LeaveCover);
        assert_eq!(parse("Leave Cover"), Command::LeaveCover);
        assert_eq!(parse("LEAVE COVER"), Command::LeaveCover);
    }

    #[test]
    fn test_take_full_cover_command() {
        assert_eq!(parse("take full cover"), Command::TakeFullCover);
        assert_eq!(parse("Take Full Cover"), Command::TakeFullCover);
        assert_eq!(parse("TAKE FULL COVER"), Command::TakeFullCover);
    }

    #[test]
    fn test_take_cover_still_works() {
        assert_eq!(parse("take cover"), Command::TakeCover);
        assert_eq!(parse("Take Cover"), Command::TakeCover);
    }

    // ---- Climb command ----

    #[test]
    fn test_climb_command() {
        assert_eq!(parse("climb chains"), Command::Climb("chains".to_string()));
        assert_eq!(parse("climb bookshelf"), Command::Climb("bookshelf".to_string()));
        assert_eq!(parse("climb well"), Command::Climb("well".to_string()));
    }

    #[test]
    fn test_climb_bare_verb_error() {
        match parse("climb") {
            Command::Unknown(s) => assert!(s.to_lowercase().contains("what"), "Got: {}", s),
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_climb_case_insensitive() {
        assert_eq!(parse("CLIMB CHAINS"), Command::Climb("chains".to_string()));
        assert_eq!(parse("Climb Bookshelf"), Command::Climb("bookshelf".to_string()));
    }

    // ---- Action Surge (Fighter level 2, issue #278) ----

    #[test]
    fn test_action_surge_command() {
        assert_eq!(parse("action surge"), Command::ActionSurge);
    }

    #[test]
    fn test_action_surge_alias_surge() {
        assert_eq!(parse("surge"), Command::ActionSurge);
    }

    #[test]
    fn test_action_surge_case_insensitive() {
        assert_eq!(parse("ACTION SURGE"), Command::ActionSurge);
        assert_eq!(parse("Action Surge"), Command::ActionSurge);
        assert_eq!(parse("SURGE"), Command::ActionSurge);
        assert_eq!(parse("Surge"), Command::ActionSurge);
    }

    #[test]
    fn test_reckless_attack_two_word() {
        assert_eq!(
            parse("reckless attack goblin"),
            Command::RecklessAttack("goblin".to_string())
        );
    }

    #[test]
    fn test_reckless_attack_adverb_form() {
        assert_eq!(
            parse("recklessly attack goblin"),
            Command::RecklessAttack("goblin".to_string())
        );
    }

    #[test]
    fn test_reckless_attack_no_target() {
        match parse("reckless attack") {
            Command::Unknown(_) => {}
            other => panic!("Expected Unknown, got {:?}", other),
        }
    }

    #[test]
    fn test_reckless_attack_case_insensitive() {
        assert_eq!(
            parse("RECKLESS ATTACK Goblin"),
            Command::RecklessAttack("goblin".to_string())
        );
        assert_eq!(
            parse("Recklessly Attack Orc"),
            Command::RecklessAttack("orc".to_string())
        );
    }

    // ---- Buffs / Conditions / Effects ----

    #[test]
    fn test_buffs_command_aliases() {
        assert_eq!(parse("buffs"), Command::Buffs);
        assert_eq!(parse("conditions"), Command::Buffs);
        assert_eq!(parse("effects"), Command::Buffs);
    }

    #[test]
    fn test_buffs_command_case_insensitive() {
        assert_eq!(parse("BUFFS"), Command::Buffs);
        assert_eq!(parse("Conditions"), Command::Buffs);
        assert_eq!(parse("Effects"), Command::Buffs);
    }

    #[test]
    fn test_active_effects_two_word_alias() {
        assert_eq!(parse("active effects"), Command::Buffs);
    }
}
