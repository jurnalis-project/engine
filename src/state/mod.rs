// jurnalis-engine/src/state/mod.rs
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::types::{LocationId, NpcId, ItemId, TriggerId, Direction};
use crate::character::Character;
use crate::conditions::ActiveCondition;

pub const SAVE_VERSION: &str = "0.1.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    pub id: String,
    pub title: String,
    pub description: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectiveType {
    DefeatNpc(NpcId),
    FindItem(ItemId),
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProgressState {
    pub first_victory: bool,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    #[serde(default)]
    pub objective_triggers: Vec<ObjectiveType>,
}

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
    pub active_combat: Option<crate::combat::CombatState>,
    #[serde(default)]
    pub ironman_mode: bool,
    #[serde(default)]
    pub progress: ProgressState,
    /// Monotonic in-world time counter in minutes. Advanced by rest, travel,
    /// and other time-passing actions. Starts at 0.
    #[serde(default)]
    pub in_world_minutes: u64,
    /// `in_world_minutes` at the start of the most recent completed long rest,
    /// used to enforce the SRD 5.1 "one long rest per 24 hours" rule.
    /// `None` if the character has never taken a long rest.
    #[serde(default)]
    pub last_long_rest_minutes: Option<u64>,
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
    pub combat_stats: Option<CombatStats>,
    #[serde(default)]
    pub conditions: Vec<ActiveCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombatStats {
    pub max_hp: i32,
    pub current_hp: i32,
    pub ac: i32,
    pub speed: i32,
    pub ability_scores: HashMap<crate::types::Ability, i32>,
    pub attacks: Vec<NpcAttack>,
    pub proficiency_bonus: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcAttack {
    pub name: String,
    pub hit_bonus: i32,
    pub damage_dice: u32,
    pub damage_die: u32,
    pub damage_bonus: i32,
    pub damage_type: DamageType,
    pub reach: u32,
    pub range_normal: u32,
    pub range_long: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamageType { Slashing, Piercing, Bludgeoning, Fire, Force }

impl std::fmt::Display for DamageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DamageType::Slashing => write!(f, "slashing"),
            DamageType::Piercing => write!(f, "piercing"),
            DamageType::Bludgeoning => write!(f, "bludgeoning"),
            DamageType::Fire => write!(f, "fire"),
            DamageType::Force => write!(f, "force"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeaponCategory { Simple, Martial }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmorCategory { Light, Medium, Heavy, Shield }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ItemType {
    Weapon {
        damage_dice: u32,
        damage_die: u32,
        damage_type: DamageType,
        properties: u16,
        category: WeaponCategory,
        versatile_die: u32,
        range_normal: u32,
        range_long: u32,
    },
    Armor {
        category: ArmorCategory,
        base_ac: u32,
        max_dex_bonus: Option<u32>,
        str_requirement: u32,
        stealth_disadvantage: bool,
    },
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
    #[serde(default)]
    pub damage_on_failure: i32,
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
    Victory,
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
            active_combat: None,
            ironman_mode: false,
            progress: ProgressState::default(),
            in_world_minutes: 0,
            last_long_rest_minutes: None,
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

    #[test]
    fn test_load_game_missing_ironman_mode_defaults_false() {
        let state = test_state();
        let mut json: serde_json::Value = serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("ironman_mode");

        let loaded = load_game(&json.to_string()).unwrap();
        assert!(!loaded.ironman_mode);
    }

    #[test]
    fn test_weapon_item_type_has_srd_fields() {
        let weapon = ItemType::Weapon {
            damage_dice: 1,
            damage_die: 8,
            damage_type: DamageType::Slashing,
            properties: 0,
            category: WeaponCategory::Martial,
            versatile_die: 10,
            range_normal: 0,
            range_long: 0,
        };
        match weapon {
            ItemType::Weapon { damage_die, damage_type, category, .. } => {
                assert_eq!(damage_die, 8);
                assert_eq!(damage_type, DamageType::Slashing);
                assert_eq!(category, WeaponCategory::Martial);
            }
            _ => panic!("Expected Weapon"),
        }
    }

    #[test]
    fn test_armor_item_type_has_srd_fields() {
        let armor = ItemType::Armor {
            category: ArmorCategory::Heavy,
            base_ac: 16,
            max_dex_bonus: Some(0),
            str_requirement: 13,
            stealth_disadvantage: true,
        };
        match armor {
            ItemType::Armor { base_ac, category, stealth_disadvantage, .. } => {
                assert_eq!(base_ac, 16);
                assert_eq!(category, ArmorCategory::Heavy);
                assert!(stealth_disadvantage);
            }
            _ => panic!("Expected Armor"),
        }
    }

    #[test]
    fn test_fire_and_force_damage_types() {
        assert_eq!(DamageType::Fire.to_string(), "fire");
        assert_eq!(DamageType::Force.to_string(), "force");
        // Verify serialization round-trip
        let json = serde_json::to_string(&DamageType::Fire).unwrap();
        let loaded: DamageType = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, DamageType::Fire);
        let json = serde_json::to_string(&DamageType::Force).unwrap();
        let loaded: DamageType = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, DamageType::Force);
    }

    #[test]
    fn test_load_game_missing_progress_defaults() {
        let state = test_state();
        let mut json: serde_json::Value = serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("progress");

        let loaded = load_game(&json.to_string()).unwrap();
        assert!(!loaded.progress.first_victory);
    }

    #[test]
    fn test_objective_struct_fields() {
        let obj = Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat the Boss".to_string(),
            description: "Slay the fearsome enemy.".to_string(),
            completed: false,
        };
        assert_eq!(obj.id, "defeat_boss");
        assert_eq!(obj.title, "Defeat the Boss");
        assert!(!obj.completed);
    }

    #[test]
    fn test_objective_type_variants() {
        let defeat = ObjectiveType::DefeatNpc(42);
        let find = ObjectiveType::FindItem(7);
        match defeat {
            ObjectiveType::DefeatNpc(id) => assert_eq!(id, 42),
            _ => panic!("Expected DefeatNpc"),
        }
        match find {
            ObjectiveType::FindItem(id) => assert_eq!(id, 7),
            _ => panic!("Expected FindItem"),
        }
    }

    #[test]
    fn test_progress_state_has_objectives() {
        let progress = ProgressState::default();
        assert!(progress.objectives.is_empty());
        assert!(progress.objective_triggers.is_empty());
    }

    #[test]
    fn test_objective_serialization_roundtrip() {
        let obj = Objective {
            id: "find_artifact".to_string(),
            title: "Find the Ancient Gem".to_string(),
            description: "Locate the gem hidden in the ruins.".to_string(),
            completed: true,
        };
        let json = serde_json::to_string(&obj).unwrap();
        let loaded: Objective = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.id, "find_artifact");
        assert!(loaded.completed);
    }

    #[test]
    fn test_progress_with_objectives_save_load() {
        let mut state = test_state();
        state.progress.objectives.push(Objective {
            id: "defeat_boss".to_string(),
            title: "Defeat Theron".to_string(),
            description: "Slay Theron the Scarred.".to_string(),
            completed: false,
        });
        state.progress.objective_triggers.push(ObjectiveType::DefeatNpc(3));

        let json = save_game(&state).unwrap();
        let loaded = load_game(&json).unwrap();
        assert_eq!(loaded.progress.objectives.len(), 1);
        assert_eq!(loaded.progress.objectives[0].title, "Defeat Theron");
        assert_eq!(loaded.progress.objective_triggers.len(), 1);
    }

    #[test]
    fn test_game_phase_victory_variant_exists() {
        let phase = GamePhase::Victory;
        assert_eq!(phase, GamePhase::Victory);
        // Verify it serializes/deserializes
        let json = serde_json::to_string(&phase).unwrap();
        let loaded: GamePhase = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, GamePhase::Victory);
    }

    #[test]
    fn test_new_state_has_zero_in_world_time() {
        let state = test_state();
        assert_eq!(state.in_world_minutes, 0);
        assert_eq!(state.last_long_rest_minutes, None);
    }

    #[test]
    fn test_load_game_missing_rest_fields_defaults() {
        let state = test_state();
        let mut json: serde_json::Value = serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("in_world_minutes");
        json.as_object_mut().unwrap().remove("last_long_rest_minutes");

        let loaded = load_game(&json.to_string()).unwrap();
        assert_eq!(loaded.in_world_minutes, 0);
        assert_eq!(loaded.last_long_rest_minutes, None);
    }

    #[test]
    fn test_load_game_missing_objectives_defaults_empty() {
        let state = test_state();
        let mut json: serde_json::Value = serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        // Remove objectives and objective_triggers from progress
        if let Some(progress) = json.get_mut("progress").and_then(|p| p.as_object_mut()) {
            progress.remove("objectives");
            progress.remove("objective_triggers");
        }

        let loaded = load_game(&json.to_string()).unwrap();
        assert!(loaded.progress.objectives.is_empty());
        assert!(loaded.progress.objective_triggers.is_empty());
    }
}
