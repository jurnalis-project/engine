// jurnalis-engine/src/state/mod.rs
use crate::character::Character;
use crate::combat::monsters::{default_multiattack, CreatureType, Size};
use crate::conditions::{ActiveCondition, ConditionType};
use crate::types::{Alignment, Direction, ItemId, LocationId, NpcId, TriggerId};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    /// Transient state used during character creation: the ability-adjustment
    /// pattern the player picked for their background (1 = +2/+1, 2 = +1/+1/+1).
    /// Consumed and cleared at character finalization (ChooseName step).
    #[serde(default)]
    pub pending_background_pattern: Option<u8>,
    /// Transient state used during character creation: the subrace/lineage
    /// the player chose for their species. Consumed at character finalization
    /// and stored on the Character. `None` for species without subraces.
    #[serde(default)]
    pub pending_subrace: Option<String>,
    /// Transient flag set when the player types `new game` during active play.
    /// Requires a "yes" confirmation to reinitialize. Cleared on any input
    /// that resolves the prompt. `#[serde(default)]` for forward-compatibility
    /// with saves produced before this field existed.
    #[serde(default)]
    pub pending_new_game_confirm: bool,
    /// Transient state used after the engine emits a disambiguation prompt.
    /// Carries the verb prefix (e.g. "take", "equip off hand") and the exact
    /// candidate names the prompt listed, in display order. When set, the
    /// orchestrator reroutes a numeric input ("1", "2", ...) into the
    /// original command with the chosen candidate substituted. Any other
    /// input clears this field before normal parsing runs. See
    /// `docs/specs/command-parser.md` (§ Disambiguation).
    #[serde(default)]
    pub pending_disambiguation: Option<PendingDisambiguation>,
}

/// Carries the context needed to resolve a numeric selection after a
/// disambiguation prompt. `verb_prefix` is the command head to re-apply
/// (e.g. `"take"`, `"talk to"`, `"equip"`); `candidates` is the ordered list
/// of exact entity names the prompt displayed (1-indexed to the player);
/// `verb_suffix` is an optional trailing modifier that sits AFTER the
/// candidate name (e.g. `"off hand"` for `equip <weapon> off hand`). The
/// orchestrator reconstructs the resolved input as
/// `"{verb_prefix} {candidates[n-1]} {verb_suffix}"`, trimming any empty
/// segments, and re-dispatches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingDisambiguation {
    pub verb_prefix: String,
    pub candidates: Vec<String>,
    #[serde(default)]
    pub verb_suffix: String,
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
    #[serde(default)]
    pub room_features: Vec<RoomFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomFeature {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocationType {
    Room,
    Corridor,
    Cave,
    Clearing,
    Ruins,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightLevel {
    Bright,
    Dim,
    Dark,
}

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
    /// SRD challenge rating, persisted on the NPC so the orchestrator can
    /// award XP on combat victory (see `leveling::xp_for_cr`). Defaults to
    /// 0.0 for older saves that pre-date this field; CR 0 maps to 10 XP.
    #[serde(default)]
    pub cr: f32,
    /// Full SRD stat-block fields (see `docs/specs/monster-stat-blocks.md`).
    /// All `#[serde(default)]` for backwards compatibility with older saves
    /// that pre-date these fields.
    #[serde(default)]
    pub creature_type: CreatureType,
    #[serde(default)]
    pub size: Size,
    #[serde(default)]
    pub alignment: Alignment,
    #[serde(default)]
    pub damage_resistances: Vec<DamageType>,
    #[serde(default)]
    pub damage_immunities: Vec<DamageType>,
    #[serde(default)]
    pub condition_immunities: Vec<ConditionType>,
    #[serde(default)]
    pub senses: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    /// Number of attacks per Attack action. Defaults to 1 for older saves.
    #[serde(default = "default_multiattack")]
    pub multiattack: u32,
    /// Free-form special traits as `(name, description)` pairs.
    #[serde(default)]
    pub special_traits: Vec<(String, String)>,
}

impl Default for CombatStats {
    fn default() -> Self {
        CombatStats {
            max_hp: 0,
            current_hp: 0,
            ac: 0,
            speed: 0,
            ability_scores: HashMap::new(),
            attacks: Vec::new(),
            proficiency_bonus: 0,
            cr: 0.0,
            creature_type: CreatureType::default(),
            size: Size::default(),
            alignment: Alignment::default(),
            damage_resistances: Vec::new(),
            damage_immunities: Vec::new(),
            condition_immunities: Vec::new(),
            senses: Vec::new(),
            languages: Vec::new(),
            multiattack: default_multiattack(),
            special_traits: Vec::new(),
        }
    }
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
pub enum NpcRole {
    Merchant,
    Guard,
    Hermit,
    Adventurer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Disposition {
    Friendly,
    Neutral,
    Hostile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: ItemId,
    pub name: String,
    pub description: String,
    pub item_type: ItemType,
    pub location: Option<LocationId>,
    pub carried_by_player: bool,
    /// Remaining charges for `ItemType::Wand` items. `None` for all
    /// non-wand items. Defaults to `None` for back-compat with older saves.
    #[serde(default)]
    pub charges_remaining: Option<u32>,
}

impl Default for Item {
    fn default() -> Self {
        Item {
            id: 0,
            name: String::new(),
            description: String::new(),
            item_type: ItemType::Misc,
            location: None,
            carried_by_player: false,
            charges_remaining: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DamageType {
    Slashing,
    Piercing,
    Bludgeoning,
    Fire,
    Force,
    // Added 2026-04-15 (monster-stat-blocks): full SRD damage-type set so
    // monster damage immunities/resistances can express the canonical types.
    Acid,
    Cold,
    Lightning,
    Necrotic,
    Poison,
    Psychic,
    Radiant,
    Thunder,
}

impl std::fmt::Display for DamageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DamageType::Slashing => write!(f, "slashing"),
            DamageType::Piercing => write!(f, "piercing"),
            DamageType::Bludgeoning => write!(f, "bludgeoning"),
            DamageType::Fire => write!(f, "fire"),
            DamageType::Force => write!(f, "force"),
            DamageType::Acid => write!(f, "acid"),
            DamageType::Cold => write!(f, "cold"),
            DamageType::Lightning => write!(f, "lightning"),
            DamageType::Necrotic => write!(f, "necrotic"),
            DamageType::Poison => write!(f, "poison"),
            DamageType::Psychic => write!(f, "psychic"),
            DamageType::Radiant => write!(f, "radiant"),
            DamageType::Thunder => write!(f, "thunder"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeaponCategory {
    Simple,
    Martial,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArmorCategory {
    Light,
    Medium,
    Heavy,
    Shield,
}

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
    Consumable {
        effect: String,
    },
    Key {
        unlocks: String,
    },
    Misc,
    // --- Magic item variants (added 2026-04-15, feat/magic-items). ---
    // All additive; pre-existing variants above are untouched so older saves
    // round-trip without modification.
    /// A magical weapon. Carries the base weapon's mechanical fields (so
    /// `resolve_player_attack` can treat it uniformly) plus flat
    /// attack/damage bonuses applied by the `lib.rs` orchestrator.
    MagicWeapon {
        base_weapon: String,
        damage_dice: u32,
        damage_die: u32,
        damage_type: DamageType,
        properties: u16,
        category: WeaponCategory,
        versatile_die: u32,
        range_normal: u32,
        range_long: u32,
        attack_bonus: i32,
        damage_bonus: i32,
        rarity: crate::equipment::magic::Rarity,
        requires_attunement: bool,
    },
    /// A magical armor. Carries the base armor's fields plus an additive
    /// AC bonus applied in `equipment::calculate_ac`.
    MagicArmor {
        base_armor: String,
        category: ArmorCategory,
        base_ac: u32,
        max_dex_bonus: Option<u32>,
        str_requirement: u32,
        stealth_disadvantage: bool,
        ac_bonus: i32,
        rarity: crate::equipment::magic::Rarity,
        requires_attunement: bool,
    },
    /// A wondrous item (no slot; effect gated by attunement).
    Wondrous {
        effect: crate::equipment::magic::WondrousEffect,
        rarity: crate::equipment::magic::Rarity,
        requires_attunement: bool,
    },
    /// A one-shot potion.
    Potion {
        effect: crate::equipment::magic::PotionEffect,
        rarity: crate::equipment::magic::Rarity,
    },
    /// A spell scroll.
    Scroll {
        spell_name: String,
        spell_level: u32,
        rarity: crate::equipment::magic::Rarity,
    },
    /// A charged wand. Remaining charges live on `Item::charges_remaining`.
    Wand {
        spell_name: String,
        rarity: crate::equipment::magic::Rarity,
        requires_attunement: bool,
    },
    /// An adventuring gear item (rope, torch, tinderbox, etc.). Added in
    /// v0.32 (feat/adventuring-gear). `#[serde(default)]`-safe because older
    /// saves never emit this variant; new items produced by world-gen carry it.
    GearItem {
        /// Canonical SRD gear name, used for display and tool-use lookup.
        gear_name: String,
        /// Weight in quarter-pounds (1 lb = 4 qp). 0 = negligible / worn.
        weight_qp: u32,
        /// Cost in copper pieces.
        cost_cp: u32,
    },
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
    /// In-play prompt to spend an unspent ASI/feat credit. Entered after a
    /// level-up if `character.asi_credits > 0`. The orchestrator returns to
    /// `Exploration` once the credit is spent. See `docs/specs/feat-system.md`.
    ChooseAsi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CreationStep {
    ChooseRace,
    /// Select a subrace/lineage for species that have one (Elf, Dragonborn,
    /// Gnome, Goliath, Tiefling). Inserted after ChooseRace when applicable;
    /// species without subraces skip directly to ChooseClass.
    ChooseSubrace,
    ChooseClass,
    /// New: select background (between ChooseClass and ChooseAbilityMethod).
    ChooseBackground,
    /// New: select origin feat (between ChooseBackground and
    /// ChooseBackgroundAbilityPattern). Defaults to background's suggestion
    /// if the player accepts.
    ChooseOriginFeat,
    /// New: select ability adjustment pattern (+2/+1 or +1/+1/+1).
    ChooseBackgroundAbilityPattern,
    ChooseAbilityMethod,
    PointBuy,
    AssignAbilities,
    ChooseSkills,
    /// New (#35): SRD alignment selection. Sits between ChooseSkills and
    /// ChooseName so it mirrors the SRD creation order (background/species/
    /// abilities -> alignment -> details).
    ChooseAlignment,
    ChooseName,
    /// Wizard-specific: choose 6 level-1 spells for the spellbook from the
    /// full Wizard level-1 spell list. Inserted after ChooseClass when the
    /// player picks Wizard. See docs/specs/wizard-spell-selection.md.
    ChooseWizardSpellbook,
    /// Wizard-specific: choose prepared spells (INT mod + level) from the
    /// spellbook just assembled. Follows ChooseWizardSpellbook immediately.
    ChooseWizardPreparedSpells,
}

pub fn save_game(state: &GameState) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(state)
}

pub fn load_game(json: &str) -> Result<GameState, String> {
    let state: GameState =
        serde_json::from_str(json).map_err(|e| format!("Failed to load save: {}", e))?;
    if state.version != SAVE_VERSION {
        return Err(format!(
            "Save version mismatch: expected {}, got {}",
            SAVE_VERSION, state.version
        ));
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::{class::Class, create_character, race::Race};
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
            "TestHero".to_string(),
            Race::Human,
            Class::Fighter,
            scores,
            vec![],
        );

        let mut locations = HashMap::new();
        locations.insert(
            0,
            Location {
                id: 0,
                name: "Test Room".to_string(),
                description: "A plain test chamber.".to_string(),
                location_type: LocationType::Room,
                exits: HashMap::new(),
                npcs: Vec::new(),
                items: Vec::new(),
                triggers: Vec::new(),
                light_level: LightLevel::Bright,
                room_features: Vec::new(),
            },
        );

        GameState {
            version: SAVE_VERSION.to_string(),
            character,
            current_location: 0,
            discovered_locations: HashSet::new(),
            world: WorldState {
                locations,
                npcs: HashMap::new(),
                items: HashMap::new(),
                triggers: HashMap::new(),
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
            pending_background_pattern: None,
            pending_subrace: None,
            pending_disambiguation: None,
            pending_new_game_confirm: false,
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
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
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
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
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
            ItemType::Weapon {
                damage_die,
                damage_type,
                category,
                ..
            } => {
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
            ItemType::Armor {
                base_ac,
                category,
                stealth_disadvantage,
                ..
            } => {
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
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("progress");

        let loaded = load_game(&json.to_string()).unwrap();
        assert!(!loaded.progress.first_victory);
    }

    #[test]
    fn test_load_game_missing_room_features_defaults_empty() {
        let state = test_state();
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        if let Some(location) = json
            .get_mut("world")
            .and_then(|world| world.get_mut("locations"))
            .and_then(|locations| locations.as_object_mut())
            .and_then(|locations| locations.get_mut("0"))
            .and_then(|location| location.as_object_mut())
        {
            location.remove("room_features");
        }

        let loaded = load_game(&json.to_string()).unwrap();
        let location = loaded.world.locations.get(&0).unwrap();
        assert!(location.room_features.is_empty());
    }

    #[test]
    fn test_item_charges_remaining_defaults_none_when_missing() {
        // Older saves without the `charges_remaining` field should
        // deserialize to None. Constructs a minimal Item JSON without the
        // field.
        let json = r#"{
            "id": 42,
            "name": "Old Trinket",
            "description": "",
            "item_type": "Misc",
            "location": null,
            "carried_by_player": true
        }"#;
        let item: Item = serde_json::from_str(json).unwrap();
        assert_eq!(item.charges_remaining, None);
    }

    #[test]
    fn test_item_charges_remaining_roundtrips_some() {
        let item = Item {
            id: 7,
            name: "Wand of Magic Missiles".to_string(),
            description: "A slim wand.".to_string(),
            item_type: ItemType::Wand {
                spell_name: "Magic Missile".to_string(),
                rarity: crate::equipment::magic::Rarity::Uncommon,
                requires_attunement: false,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: Some(5),
        };
        let json = serde_json::to_string(&item).unwrap();
        let loaded: Item = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.charges_remaining, Some(5));
        match loaded.item_type {
            ItemType::Wand {
                ref spell_name,
                requires_attunement,
                ..
            } => {
                assert_eq!(spell_name, "Magic Missile");
                assert!(!requires_attunement);
            }
            _ => panic!("expected Wand"),
        }
    }

    #[test]
    fn test_magic_weapon_item_type_roundtrips() {
        use crate::equipment::magic::Rarity;
        let item = Item {
            id: 3,
            name: "+2 Longsword".to_string(),
            description: "A magic sword.".to_string(),
            item_type: ItemType::MagicWeapon {
                base_weapon: "Longsword".to_string(),
                damage_dice: 1,
                damage_die: 8,
                damage_type: DamageType::Slashing,
                properties: 0,
                category: WeaponCategory::Martial,
                versatile_die: 10,
                range_normal: 0,
                range_long: 0,
                attack_bonus: 2,
                damage_bonus: 2,
                rarity: Rarity::Rare,
                requires_attunement: false,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        let loaded: Item = serde_json::from_str(&json).unwrap();
        match loaded.item_type {
            ItemType::MagicWeapon {
                attack_bonus,
                damage_bonus,
                rarity,
                ..
            } => {
                assert_eq!(attack_bonus, 2);
                assert_eq!(damage_bonus, 2);
                assert_eq!(rarity, Rarity::Rare);
            }
            _ => panic!("expected MagicWeapon"),
        }
    }

    #[test]
    fn test_magic_armor_item_type_roundtrips() {
        use crate::equipment::magic::Rarity;
        let item = Item {
            id: 4,
            name: "+1 Chain Mail".to_string(),
            description: "Enchanted chain.".to_string(),
            item_type: ItemType::MagicArmor {
                base_armor: "Chain Mail".to_string(),
                category: ArmorCategory::Heavy,
                base_ac: 16,
                max_dex_bonus: Some(0),
                str_requirement: 13,
                stealth_disadvantage: true,
                ac_bonus: 1,
                rarity: Rarity::Rare,
                requires_attunement: false,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        let loaded: Item = serde_json::from_str(&json).unwrap();
        match loaded.item_type {
            ItemType::MagicArmor {
                ac_bonus, base_ac, ..
            } => {
                assert_eq!(ac_bonus, 1);
                assert_eq!(base_ac, 16);
            }
            _ => panic!("expected MagicArmor"),
        }
    }

    #[test]
    fn test_potion_item_type_roundtrips() {
        use crate::equipment::magic::{PotionEffect, Rarity};
        let item = Item {
            id: 5,
            name: "Potion of Greater Healing".to_string(),
            description: "Vial of potent brew.".to_string(),
            item_type: ItemType::Potion {
                effect: PotionEffect::Healing {
                    dice: 4,
                    die: 4,
                    bonus: 4,
                },
                rarity: Rarity::Uncommon,
            },
            location: None,
            carried_by_player: true,
            charges_remaining: None,
        };
        let json = serde_json::to_string(&item).unwrap();
        let loaded: Item = serde_json::from_str(&json).unwrap();
        match loaded.item_type {
            ItemType::Potion { effect, rarity } => {
                assert_eq!(rarity, Rarity::Uncommon);
                match effect {
                    PotionEffect::Healing { dice, die, bonus } => {
                        assert_eq!(dice, 4);
                        assert_eq!(die, 4);
                        assert_eq!(bonus, 4);
                    }
                    _ => panic!("expected Healing"),
                }
            }
            _ => panic!("expected Potion"),
        }
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
        state
            .progress
            .objective_triggers
            .push(ObjectiveType::DefeatNpc(3));

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
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("in_world_minutes");
        json.as_object_mut()
            .unwrap()
            .remove("last_long_rest_minutes");

        let loaded = load_game(&json.to_string()).unwrap();
        assert_eq!(loaded.in_world_minutes, 0);
        assert_eq!(loaded.last_long_rest_minutes, None);
    }

    #[test]
    fn test_load_game_missing_objectives_defaults_empty() {
        let state = test_state();
        let mut json: serde_json::Value =
            serde_json::from_str(&save_game(&state).unwrap()).unwrap();
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
