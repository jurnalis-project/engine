// jurnalis-engine/src/state/mod.rs
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::types::{LocationId, NpcId, ItemId, TriggerId, Direction};
use crate::character::Character;

pub const SAVE_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub version: String,
    pub character: Character,
    pub current_location: LocationId,
    pub discovered_locations: HashSet<LocationId>,
    pub world: WorldState,
    pub log: Vec<String>,
    pub rng_seed: u64,
    pub rng_counter: u64,
    pub game_phase: GamePhase,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    pub locations: HashMap<LocationId, Location>,
    pub npcs: HashMap<NpcId, Npc>,
    pub items: HashMap<ItemId, Item>,
    pub triggers: HashMap<TriggerId, Trigger>,
    pub triggered: HashSet<TriggerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub id: LocationId,
    pub name: String,
    pub description: String,
    pub location_type: LocationType,
    pub exits: HashMap<Direction, LocationId>,
    pub npcs: Vec<NpcId>,
    pub items: Vec<ItemId>,
    pub triggers: Vec<TriggerId>,
    pub light_level: LightLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationType { Room, Corridor, Cave, Clearing, Ruins }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightLevel { Bright, Dim, Dark }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Npc {
    pub id: NpcId,
    pub name: String,
    pub role: NpcRole,
    pub disposition: Disposition,
    pub dialogue_tags: Vec<String>,
    pub location: LocationId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NpcRole { Merchant, Guard, Hermit, Adventurer }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposition { Friendly, Neutral, Hostile }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub item_type: ItemType,
    pub location: Option<LocationId>,
    pub carried_by_player: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemType {
    Weapon { damage_die: u32 },
    Armor { ac_bonus: i32 },
    Consumable { effect: String },
    Key { unlocks: String },
    Misc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trigger {
    pub id: TriggerId,
    pub location: LocationId,
    pub trigger_type: TriggerType,
    pub dc: i32,
    pub success_text: String,
    pub failure_text: String,
    pub one_shot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TriggerType {
    SkillCheck(crate::types::Skill),
    SavingThrow(crate::types::Ability),
    PassivePerception,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    CharacterCreation(CreationStep),
    Exploration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreationStep {
    ChooseRace,
    ChooseClass,
    ChooseAbilityMethod,
    PointBuy,
    AssignAbilities,
    ChooseSkills,
    ChooseName,
}

pub fn save_game(state: &GameState) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(state)
}

pub fn load_game(json: &str) -> Result<GameState, String> {
    let state: GameState = serde_json::from_str(json).map_err(|e| format!("Failed to load save: {}", e))?;
    if state.version != SAVE_VERSION {
        return Err(format!("Save version mismatch: expected {}, got {}", SAVE_VERSION, state.version));
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{create_character, race::Race, class::Class};
    use crate::types::Ability;

    fn test_state() -> GameState {
        let mut scores = HashMap::new();
        scores.insert(Ability::Strength, 15);
        scores.insert(Ability::Dexterity, 14);
        scores.insert(Ability::Constitution, 13);
        scores.insert(Ability::Intelligence, 12);
        scores.insert(Ability::Wisdom, 10);
        scores.insert(Ability::Charisma, 8);

        let character = create_character(
            "TestHero".to_string(), Race::Human, Class::Fighter, scores, vec![],
        );

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations: HashMap::new(), npcs: HashMap::new(),
                items: HashMap::new(), triggers: HashMap::new(),
                triggered: HashSet::new(),
            },
            log: Vec::new(),
            rng_seed: 42,
            rng_counter: 0,
            game_phase: GamePhase::Exploration,
        }
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let state = test_state();
        let json = save_game(&state).unwrap();
        let loaded = load_game(&json).unwrap();
        assert_eq!(loaded.character.name, "TestHero");
        assert_eq!(loaded.rng_seed, 42);
        assert_eq!(loaded.version, SAVE_VERSION);
    }

    #[test]
    fn test_load_wrong_version() {
        let state = test_state();
        let mut json: serde_json::Value = serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json["version"] = serde_json::Value::String("99.0.0".to_string());
        let result = load_game(&json.to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("version mismatch"));
    }

    #[test]
    fn test_load_invalid_json() {
        let result = load_game("not valid json");
        assert!(result.is_err());
    }
}
