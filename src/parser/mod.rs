// jurnalis-engine/src/parser/mod.rs
use crate::types::{Direction, Skill};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Look(Option<String>),
    Go(Direction),
    Talk(String),
    Take(String),
    Use(String),
    Inventory,
    CharacterSheet,
    Check(String),
    Save(Option<String>),
    Load(Option<String>),
    Help(Option<String>),
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let input = input.trim();
    if input.is_empty() {
        return Command::Unknown(String::new());
    }

    let lower = input.to_lowercase();
    let parts: Vec<&str> = lower.split_whitespace().collect();
    let verb = parts[0];
    let args = if parts.len() > 1 {
        parts[1..].join(" ")
    } else {
        String::new()
    };

    match verb {
        "look" | "l" | "examine" => {
            if args.is_empty() { Command::Look(None) } else { Command::Look(Some(args)) }
        }
        "go" => parse_direction(&args).map(Command::Go).unwrap_or(Command::Unknown(input.to_string())),
        "n" | "north" => Command::Go(Direction::North),
        "s" | "south" => Command::Go(Direction::South),
        "e" | "east" => Command::Go(Direction::East),
        "w" | "west" => Command::Go(Direction::West),
        "u" | "up" => Command::Go(Direction::Up),
        "d" | "down" => Command::Go(Direction::Down),
        "talk" | "speak" => {
            if args.is_empty() { Command::Unknown("Talk to whom?".to_string()) } else { Command::Talk(args) }
        }
        "take" | "get" => {
            if args.is_empty() { Command::Unknown("Take what?".to_string()) } else { Command::Take(args) }
        }
        "pick" if parts.get(1) == Some(&"up") => {
            let rest = if parts.len() > 2 { parts[2..].join(" ") } else { String::new() };
            if rest.is_empty() { Command::Unknown("Pick up what?".to_string()) } else { Command::Take(rest) }
        }
        "use" => {
            if args.is_empty() { Command::Unknown("Use what?".to_string()) } else { Command::Use(args) }
        }
        "inventory" | "i" | "inv" => Command::Inventory,
        "character" | "char" | "sheet" => Command::CharacterSheet,
        "check" => {
            if args.is_empty() { Command::Unknown("Check which skill?".to_string()) } else { Command::Check(args) }
        }
        "save" => { if args.is_empty() { Command::Save(None) } else { Command::Save(Some(args)) } }
        "load" => { if args.is_empty() { Command::Load(None) } else { Command::Load(Some(args)) } }
        "help" | "?" => { if args.is_empty() { Command::Help(None) } else { Command::Help(Some(args)) } }
        _ => Command::Unknown(input.to_string()),
    }
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
}
