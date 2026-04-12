// jurnalis-engine/src/parser/mod.rs
pub mod resolver;

use crate::types::{Direction, Skill};

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Look(Option<String>),
    Go(Direction),
    Talk(String),
    Take(String),
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
    // Combat commands
    Attack(String),
    Approach(String),
    Retreat,
    Dodge,
    Disengage,
    Dash,
    Unknown(String),
}

pub fn parse(input: &str) -> Command {
    let input = input.trim();
    if input.is_empty() {
        return Command::Unknown(String::new());
    }

    let lower = input.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();

    // 2-word phrases first
    if words.len() >= 2 {
        let two = format!("{} {}", words[0], words[1]);
        let rest = if words.len() > 2 { words[2..].join(" ") } else { String::new() };

        match two.as_str() {
            "look at" | "check out" => {
                return if rest.is_empty() { Command::Look(None) } else { Command::Look(Some(rest)) };
            }
            "look around" => return Command::Look(None),
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
            _ => {}
        }
    }

    // 1-word verbs
    let verb = words[0];
    let args = if words.len() > 1 { words[1..].join(" ") } else { String::new() };

    match verb {
        "look" | "l" | "examine" | "inspect" | "see" | "search" => {
            if args.is_empty() { Command::Look(None) } else { Command::Look(Some(args)) }
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
            if args.is_empty() { Command::Unknown("Take what?".to_string()) } else { Command::Take(args) }
        }
        "drop" | "discard" => {
            if args.is_empty() { Command::Unknown("Drop what?".to_string()) } else { Command::Drop(args) }
        }
        "use" | "activate" | "apply" => {
            if args.is_empty() { Command::Unknown("Use what?".to_string()) } else { Command::Use(args) }
        }
        "equip" | "wear" | "wield" | "don" => {
            if args.is_empty() { Command::Unknown("Equip what?".to_string()) } else { Command::Equip(args) }
        }
        "unequip" | "doff" => {
            if args.is_empty() { Command::Unknown("Unequip what?".to_string()) } else { Command::Unequip(args) }
        }
        "attack" | "hit" | "strike" | "shoot" => {
            if args.is_empty() { Command::Unknown("Attack what?".to_string()) } else { Command::Attack(args) }
        }
        "approach" | "advance" | "close" => {
            if args.is_empty() { Command::Unknown("Approach what?".to_string()) } else { Command::Approach(args) }
        }
        "retreat" => Command::Retreat,
        "dodge" => Command::Dodge,
        "disengage" | "withdraw" => Command::Disengage,
        "dash" | "run" | "sprint" => Command::Dash,
        "end" | "pass" | "wait" => Command::EndTurn,
        "inventory" | "i" | "inv" | "items" | "bag" => Command::Inventory,
        "character" | "char" | "sheet" | "stats" | "status" => Command::CharacterSheet,
        "check" | "roll" | "try" => {
            if args.is_empty() { Command::Unknown("Check which skill?".to_string()) } else { Command::Check(args) }
        }
        "save" => { if args.is_empty() { Command::Save(None) } else { Command::Save(Some(args)) } }
        "load" | "restore" => { if args.is_empty() { Command::Load(None) } else { Command::Load(Some(args)) } }
        "help" | "?" | "commands" => { if args.is_empty() { Command::Help(None) } else { Command::Help(Some(args)) } }
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
        assert_eq!(parse("search room"), Command::Look(Some("room".to_string())));
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

    #[test]
    fn test_attack_command() {
        assert_eq!(parse("attack goblin"), Command::Attack("goblin".to_string()));
        assert_eq!(parse("hit orc"), Command::Attack("orc".to_string()));
        assert_eq!(parse("strike skeleton"), Command::Attack("skeleton".to_string()));
        assert_eq!(parse("shoot goblin"), Command::Attack("goblin".to_string()));
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
    fn test_take_off_vs_take() {
        // "take off" -> Unequip, "take" -> Take
        assert_eq!(parse("take off helmet"), Command::Unequip("helmet".to_string()));
        assert_eq!(parse("take sword"), Command::Take("sword".to_string()));
    }
}
