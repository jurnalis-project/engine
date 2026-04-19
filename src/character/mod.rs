// jurnalis-engine/src/character/mod.rs
pub mod race;
pub mod class;
pub mod background;
pub mod feat;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;
use crate::types::{Ability, Alignment, Skill, ItemId};
use self::race::Race;
use self::class::{Class, ClassFeatureState};
use self::background::Background;
use self::feat::{FeatDef, FeatEffect};
use crate::rules::dice::roll_4d6_drop_lowest;
use crate::equipment::Equipment;
use crate::conditions::ActiveCondition;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Character {
    pub name: String,
    pub race: Race,
    pub class: Class,
    pub level: u32,
    pub ability_scores: HashMap<Ability, i32>,
    pub skill_proficiencies: Vec<Skill>,
    pub save_proficiencies: Vec<Ability>,
    pub max_hp: i32,
    pub current_hp: i32,
    pub inventory: Vec<ItemId>,
    pub speed: i32,
    pub traits: Vec<String>,
    pub equipped: Equipment,
    #[serde(default)]
    pub conditions: Vec<ActiveCondition>,
    #[serde(default)]
    pub spell_slots_max: Vec<i32>,
    #[serde(default)]
    pub spell_slots_remaining: Vec<i32>,
    #[serde(default)]
    pub known_spells: Vec<String>,
    /// Hit dice available to spend during short rest. Replenished (partially)
    /// on long rest. Max = character level.
    #[serde(default)]
    pub hit_dice_remaining: u32,
    /// Per-class feature flags tracking short-rest / long-rest resources.
    #[serde(default)]
    pub class_features: ClassFeatureState,
    /// Exhaustion level (0..=6 per SRD 5.1). Long rest reduces by 1.
    #[serde(default)]
    pub exhaustion: u32,
    /// Total accumulated experience points. Drives level advancement
    /// (see `leveling/`). Defaults to 0 for save back-compat.
    #[serde(default)]
    pub xp: u32,
    /// Number of unspent Ability Score Improvement (or feat) credits earned
    /// at SRD-mandated levels (4/8/12/16/19). Consumed by the future feat
    /// system (#28). Defaults to 0.
    #[serde(default)]
    pub asi_credits: u32,
    /// Character background. `#[serde(default)]` so older saves deserialize
    /// cleanly (defaults to `Background::Acolyte`).
    #[serde(default)]
    pub background: Background,
    /// Tool proficiency names (granted by background). Stored as strings
    /// until a tool system is modelled (pending issue #42).
    #[serde(default)]
    pub tool_proficiencies: Vec<String>,
    /// Known languages. Common is always included; additional languages
    /// come from background, race, and other features.
    #[serde(default)]
    pub languages: Vec<String>,
    /// IDs of magic items the character is currently attuned to. Capped at
    /// `equipment::magic::MAX_ATTUNED_ITEMS` (3 per SRD 5.1). Items are
    /// attuned via the `attune` command and released via `unattune`.
    /// `#[serde(default)]` so older saves without this field deserialize
    /// to an empty vec. Added 2026-04-15 (feat/magic-items).
    #[serde(default)]
    pub attuned_items: Vec<ItemId>,
    /// The origin feat chosen at character creation. Defaults to None on
    /// legacy saves and before the ChooseOriginFeat step completes.
    /// See `docs/specs/feat-system.md`.
    #[serde(default)]
    pub origin_feat: Option<String>,
    /// Names of general and fighting-style feats taken via ASI credits.
    /// Empty for characters who have never spent an ASI on a feat.
    #[serde(default)]
    pub general_feats: Vec<String>,
    /// SRD alignment. Chosen during the `ChooseAlignment` creation step.
    /// Defaults to `Alignment::Unaligned` for legacy saves predating the
    /// field and for the placeholder character allocated before character
    /// creation completes. See `docs/specs/character-system.md`.
    #[serde(default)]
    pub alignment: Alignment,
    /// Canonical SRD weapon names (e.g. "Longsword") for which the
    /// character has unlocked the 2024 SRD Weapon Mastery property. Filled
    /// at character creation from the starting loadout for mastery
    /// classes (Fighter/Barbarian/Paladin/Ranger). Empty for non-mastery
    /// classes and for legacy saves predating this field. See
    /// `docs/specs/weapon-mastery.md`.
    #[serde(default)]
    pub weapon_masteries: Vec<String>,
    /// True when the character is wearing body armor whose category is NOT in
    /// their class's `armor_proficiencies()` list. Per SRD 2024 Armor
    /// Training: imposes Disadvantage on any D20 Test using STR or DEX and
    /// blocks spellcasting. Set by `handle_equip_command` when an armor piece
    /// lands in the body slot; cleared by `handle_unequip_command` when body
    /// armor is removed. Shields are tracked separately (not yet enforced).
    /// `#[serde(default)]` so legacy saves deserialize to `false`.
    #[serde(default)]
    pub wearing_nonproficient_armor: bool,
    /// Selected subrace/lineage (e.g. "Wood Elf", "Red", "Infernal").
    /// `None` for species without subraces (Human, Dwarf, Halfling, Orc)
    /// and for legacy saves predating this field.
    #[serde(default)]
    pub subrace: Option<String>,
    /// Ammunition counts by ammunition type (e.g. "Arrow", "Bolt",
    /// "Sling Bullet", "Needle", "Bullet"). Decremented when an
    /// AMMUNITION-property weapon is used in an attack. Zero count blocks
    /// the attack. `#[serde(default)]` for backward compat.
    #[serde(default)]
    pub ammo: HashMap<String, u32>,
}

impl Character {
    pub fn proficiency_bonus(&self) -> i32 { Class::proficiency_bonus(self.level) }
    pub fn ability_modifier(&self, ability: Ability) -> i32 {
        let score = self.ability_scores.get(&ability).copied().unwrap_or(10);
        Ability::modifier(score)
    }
    pub fn is_proficient_in_skill(&self, skill: Skill) -> bool { self.skill_proficiencies.contains(&skill) }
    pub fn is_proficient_in_save(&self, ability: Ability) -> bool { self.save_proficiencies.contains(&ability) }
    pub fn skill_modifier(&self, skill: Skill) -> i32 {
        let base = self.ability_modifier(skill.ability());
        if self.is_proficient_in_skill(skill) { base + self.proficiency_bonus() } else { base }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityScoreMethod { StandardArray, PointBuy, Random }

pub const STANDARD_ARRAY: [i32; 6] = [15, 14, 13, 12, 10, 8];

pub fn generate_random_scores(rng: &mut impl Rng) -> [i32; 6] {
    let mut scores = [0i32; 6];
    for score in scores.iter_mut() { *score = roll_4d6_drop_lowest(rng); }
    scores
}

const POINT_BUY_COSTS: [(i32, i32); 8] = [
    (8, 0), (9, 1), (10, 2), (11, 3), (12, 4), (13, 5), (14, 7), (15, 9),
];

pub fn point_buy_cost(score: i32) -> Option<i32> {
    POINT_BUY_COSTS.iter().find(|(s, _)| *s == score).map(|(_, c)| *c)
}

pub fn validate_point_buy(scores: &[i32; 6]) -> Result<(), String> {
    let mut total = 0;
    for &score in scores {
        match point_buy_cost(score) {
            Some(cost) => total += cost,
            None => return Err(format!("Score {} is out of range (8-15)", score)),
        }
    }
    if total != 27 { return Err(format!("Total cost is {} (must be 27)", total)); }
    Ok(())
}

pub fn calculate_hp(class: Class, con_modifier: i32, level: u32) -> i32 {
    let hit_die = class.hit_die() as i32;
    let first_level = hit_die + con_modifier;
    let per_level = (hit_die / 2) + 1 + con_modifier;
    let additional = per_level * (level as i32 - 1);
    (first_level + additional).max(1)
}

pub fn create_character(
    name: String, race: Race, class: Class,
    ability_scores: HashMap<Ability, i32>, skill_proficiencies: Vec<Skill>,
) -> Character {
    let mut final_scores = ability_scores;
    for (ability, bonus) in race.ability_bonuses() {
        *final_scores.entry(ability).or_insert(10) += bonus;
    }
    // SRD 5.1: "None of these increases can raise a score above 20."
    for score in final_scores.values_mut() {
        *score = (*score).min(20);
    }
    let con_mod = Ability::modifier(*final_scores.get(&Ability::Constitution).unwrap_or(&10));
    let hp = calculate_hp(class, con_mod, 1);
    let save_profs = class.saving_throw_proficiencies();
    let traits = race.traits().iter().map(|s| s.to_string()).collect();

    // Starting known spells per caster class. Non-caster classes have an
    // empty list. The Wizard retains the MVP canonical list for save
    // back-compat; other casters receive a small, class-appropriate slice
    // of their SRD list (see `default_starting_spells`).
    let known_spells = default_starting_spells(class);
    let spell_slots_max = class.starting_spell_slots();
    let spell_slots_remaining = spell_slots_max.clone();

    // Initialize per-class feature state. Defaults fill the rest.
    let mut class_features = ClassFeatureState::default();
    let cha_mod = Ability::modifier(*final_scores.get(&Ability::Charisma).unwrap_or(&10));
    init_class_features(&mut class_features, class, /* level */ 1, cha_mod, &known_spells);

    // Weapon mastery (2024 SRD): fill the class's starting mastery slots
    // from the starting loadout on a best-effort basis. See
    // docs/specs/weapon-mastery.md. Duplicates are skipped so the Barbarian
    // doesn't fill both slots with "Handaxe" when its loadout contains four.
    let mut weapon_masteries: Vec<String> = Vec::new();
    let slot_count = class.starting_weapon_masteries() as usize;
    if slot_count > 0 {
        let loadout = class.starting_loadout();
        // Collect candidate weapon names in loadout order: main-hand,
        // off-hand (if a weapon), then extras. `off_hand` is skipped when
        // it's a shield rather than a weapon — `weapon_mastery` returns
        // None for shields so the filter handles this naturally.
        let mut candidates: Vec<&str> = Vec::new();
        if let Some(mh) = loadout.main_hand { candidates.push(mh); }
        if let Some(oh) = loadout.off_hand { candidates.push(oh); }
        for extra in loadout.extra_inventory.iter() { candidates.push(extra); }
        for candidate in candidates {
            if weapon_masteries.len() >= slot_count { break; }
            if crate::equipment::weapon_mastery(candidate).is_none() { continue; }
            let name = candidate.to_string();
            if !weapon_masteries.contains(&name) {
                weapon_masteries.push(name);
            }
        }
    }

    Character {
        name, race, class, level: 1,
        ability_scores: final_scores, skill_proficiencies,
        save_proficiencies: save_profs, max_hp: hp, current_hp: hp,
        inventory: Vec::new(), speed: race.speed(), traits,
        equipped: Equipment::default(),
        conditions: Vec::new(),
        spell_slots_max,
        spell_slots_remaining,
        known_spells,
        hit_dice_remaining: 1, // level 1 starts with 1 hit die
        class_features,
        exhaustion: 0,
        xp: 0,
        asi_credits: 0,
        background: Background::default(),
        tool_proficiencies: class.starting_tool_proficiencies()
            .iter()
            .map(|t| t.name().to_string())
            .collect(),
        languages: vec!["Common".to_string()],
        attuned_items: Vec::new(),
        origin_feat: None,
        general_feats: Vec::new(),
        alignment: Alignment::default(),
        weapon_masteries,
        wearing_nonproficient_armor: false,
        subrace: None,
        ammo: HashMap::new(),
    }
}

/// Starting known-spell list per caster class at level 1.
///
/// - Wizard retains the MVP canonical list for save back-compat (6 spells).
/// - Other full casters (Bard/Cleric/Druid/Sorcerer) and Warlock get a
///   small, thematic level-1 list drawn from their class spell list.
/// - Prepared casters (Cleric/Druid/Paladin/Wizard) receive this list as
///   the default prepared set (via `init_class_features`). Paladin (2024
///   SRD) starts with 2 prepared level-1 spells at character creation;
///   Ranger is out of scope for issue #86 and still starts empty.
/// - Non-caster classes return an empty vector.
///
/// The exact lists here represent a reasonable "common starter" set. A
/// future feature will let players choose known/prepared spells at
/// character creation (see docs/specs/spell-system.md).
pub fn default_starting_spells(class: Class) -> Vec<String> {
    fn v(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }
    match class {
        Class::Wizard => v(&[
            "Fire Bolt", "Prestidigitation",
            "Magic Missile", "Burning Hands", "Sleep", "Shield",
            "Charm Person", "Detect Magic",
        ]),
        Class::Sorcerer => v(&[
            "Fire Bolt", "Mage Hand",
            "Magic Missile", "Shield",
        ]),
        Class::Bard => v(&[
            "Vicious Mockery", "Mage Hand",
            "Charm Person", "Healing Word",
        ]),
        Class::Cleric => v(&[
            "Sacred Flame", "Guidance", "Light",
            "Cure Wounds", "Guiding Bolt", "Bless", "Healing Word",
        ]),
        Class::Druid => v(&[
            "Druidcraft", "Guidance",
            "Cure Wounds", "Faerie Fire", "Healing Word",
        ]),
        Class::Warlock => v(&[
            "Eldritch Blast", "Mage Hand",
            "Charm Person",
        ]),
        // Paladin (2024 SRD): gains Spellcasting at level 1 with 2 prepared
        // level-1 spells. The SRD recommends Heroism and Searing Smite;
        // Searing Smite is not in the catalog yet, so we pair Heroism with
        // Bless (both on the Paladin spell list).
        Class::Paladin => v(&["Heroism", "Bless"]),
        // Ranger still unlocks spellcasting at level 2 in this engine
        // (out of scope for issue #86).
        Class::Ranger => Vec::new(),
        Class::Barbarian | Class::Fighter | Class::Monk | Class::Rogue => Vec::new(),
    }
}

fn default_wizard_prepared_spells(known_spells: &[String]) -> Vec<String> {
    known_spells
        .iter()
        .filter(|spell| crate::spells::find_spell(spell).map(|def| def.level == 1).unwrap_or(false))
        .take(4)
        .cloned()
        .collect()
}

/// Total initiative bonus granted by the character's feats. Iterates all
/// held feats (origin + general) and sums every `FeatEffect::Initiative(n)`
/// found on their definitions. Unknown feat names are silently ignored —
/// this mirrors how `grant_starting_equipment` skips unknown items.
pub fn initiative_bonus_from_feats(character: &Character) -> i32 {
    let mut total = 0i32;
    let mut all_names: Vec<&str> = character.general_feats.iter().map(|s| s.as_str()).collect();
    if let Some(ref name) = character.origin_feat {
        all_names.push(name.as_str());
    }
    for name in all_names {
        if let Some(feat) = FeatDef::lookup(name) {
            for effect in feat.effects {
                if let FeatEffect::Initiative(n) = effect { total += *n; }
            }
        }
    }
    total
}

/// Populate per-class feature counters at character creation. Mutates the
/// already-default-initialized `ClassFeatureState`. Centralized here so the
/// orchestrator and any future entry points (level-up, respec) can call it.
pub(crate) fn init_class_features(
    features: &mut ClassFeatureState,
    class: Class,
    level: u32,
    cha_mod: i32,
    known_spells: &[String],
) {
    match class {
        Class::Barbarian => {
            features.rage_uses_remaining = match level {
                0..=2 => 2,
                3..=5 => 3,
                6..=11 => 4,
                12..=16 => 5,
                _ => 6,
            };
            features.rage_active = false;
        }
        Class::Bard => {
            features.bardic_inspiration_remaining = cha_mod.max(1) as u32;
        }
        Class::Cleric => {
            features.channel_divinity_remaining = match level {
                0..=1 => 0,
                2..=5 => 1,
                6..=17 => 2,
                _ => 3,
            };
            // Cleric is a prepared caster: prepared_spells mirrors known at creation.
            features.prepared_spells = known_spells.to_vec();
        }
        Class::Druid => {
            // Druid is a prepared caster: prepared_spells mirrors known at creation.
            features.prepared_spells = known_spells.to_vec();
        }
        Class::Paladin => {
            features.lay_on_hands_pool = 5 * level.max(1);
            features.channel_divinity_remaining = match level {
                0..=2 => 0,
                3..=10 => 1,
                11..=19 => 2,
                _ => 3,
            };
            // Paladin is a prepared caster: prepared_spells mirrors known at
            // creation (empty until level 2 when spellcasting unlocks).
            features.prepared_spells = known_spells.to_vec();
        }
        Class::Monk => {
            // Ki/Focus unlocks at level 2 in the SRD; level 1 monks have none.
            features.ki_points_remaining = if level < 2 { 0 } else { level };
        }
        Class::Wizard => {
            features.prepared_spells = default_wizard_prepared_spells(known_spells);
        }
        // Known casters (Bard/Ranger/Sorcerer/Warlock) and pure martials:
        // defaults stand.
        Class::Fighter | Class::Ranger
        | Class::Rogue | Class::Sorcerer | Class::Warlock => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_scores() -> HashMap<Ability, i32> {
        let mut m = HashMap::new();
        m.insert(Ability::Strength, 15); m.insert(Ability::Dexterity, 14);
        m.insert(Ability::Constitution, 13); m.insert(Ability::Intelligence, 12);
        m.insert(Ability::Wisdom, 10); m.insert(Ability::Charisma, 8);
        m
    }

    #[test]
    fn test_create_character_applies_racial_bonuses() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![Skill::Stealth]);
        assert_eq!(c.ability_scores[&Ability::Dexterity], 16);
    }

    #[test]
    fn test_create_character_hp() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert_eq!(c.max_hp, 12); assert_eq!(c.current_hp, 12);
    }

    #[test]
    fn test_skill_modifier_with_proficiency() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![Skill::Stealth]);
        assert_eq!(c.skill_modifier(Skill::Stealth), 5);
    }

    #[test]
    fn test_skill_modifier_without_proficiency() {
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, test_scores(), vec![]);
        assert_eq!(c.skill_modifier(Skill::Stealth), 3);
    }

    #[test]
    fn test_random_scores_in_range() {
        use rand::SeedableRng; use rand::rngs::StdRng;
        let mut rng = StdRng::seed_from_u64(42);
        let scores = generate_random_scores(&mut rng);
        for score in scores { assert!(score >= 3 && score <= 18, "Score {} out of range", score); }
    }

    #[test]
    fn test_calculate_hp_level_scaling() {
        assert_eq!(calculate_hp(Class::Fighter, 2, 1), 12);
        assert_eq!(calculate_hp(Class::Fighter, 2, 2), 20);
    }

    #[test]
    fn test_point_buy_valid() { assert!(validate_point_buy(&[15, 14, 13, 12, 10, 8]).is_ok()); }

    #[test]
    fn test_point_buy_wrong_total() { assert!(validate_point_buy(&[15, 15, 14, 8, 8, 8]).is_err()); }

    #[test]
    fn test_point_buy_out_of_range() {
        assert!(validate_point_buy(&[16, 14, 13, 12, 10, 8]).is_err());
        assert!(validate_point_buy(&[7, 14, 13, 12, 10, 8]).is_err());
    }

    #[test]
    fn test_point_buy_cost() {
        assert_eq!(point_buy_cost(8), Some(0));
        assert_eq!(point_buy_cost(15), Some(9));
        assert_eq!(point_buy_cost(16), None);
    }

    #[test]
    fn test_wizard_gets_spell_slots_and_spells() {
        let c = create_character("Gandalf".to_string(), Race::Human, Class::Wizard, test_scores(), vec![]);
        // Wizard level 1: 2 first-level slots
        assert_eq!(c.spell_slots_max, vec![2]);
        assert_eq!(c.spell_slots_remaining, vec![2]);
        // Wizard starts with 2 cantrips plus 6 spellbook spells.
        assert_eq!(c.known_spells.len(), 8);
        assert!(c.known_spells.contains(&"Fire Bolt".to_string()));
        assert!(c.known_spells.contains(&"Prestidigitation".to_string()));
        assert!(c.known_spells.contains(&"Magic Missile".to_string()));
        assert!(c.known_spells.contains(&"Burning Hands".to_string()));
        assert!(c.known_spells.contains(&"Sleep".to_string()));
        assert!(c.known_spells.contains(&"Shield".to_string()));
        assert!(c.known_spells.contains(&"Charm Person".to_string()));
        assert!(c.known_spells.contains(&"Detect Magic".to_string()));
    }

    #[test]
    fn test_fighter_has_no_spell_slots() {
        let c = create_character("Conan".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert!(c.spell_slots_max.is_empty());
        assert!(c.spell_slots_remaining.is_empty());
        assert!(c.known_spells.is_empty());
    }

    #[test]
    fn test_rogue_has_no_spell_slots() {
        let c = create_character("Shadow".to_string(), Race::Human, Class::Rogue, test_scores(), vec![]);
        assert!(c.spell_slots_max.is_empty());
        assert!(c.spell_slots_remaining.is_empty());
        assert!(c.known_spells.is_empty());
    }

    #[test]
    fn test_new_character_has_rest_fields_initialized() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        // Level 1 character should have 1 hit die available
        assert_eq!(c.hit_dice_remaining, 1);
        // All short/long rest features available
        assert!(c.class_features.second_wind_available);
        assert!(c.class_features.action_surge_available);
        assert!(!c.class_features.arcane_recovery_used_today);
        // No exhaustion at creation
        assert_eq!(c.exhaustion, 0);
    }

    #[test]
    fn test_new_character_has_background_fields_initialized() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        // Default background for unset characters
        assert_eq!(c.background, Background::Acolyte);
        // Tool proficiencies start empty; filled in at finalization
        assert!(c.tool_proficiencies.is_empty());
        // Common is always known
        assert!(c.languages.contains(&"Common".to_string()));
    }

    #[test]
    fn test_character_missing_background_fields_deserialize_defaults() {
        // Legacy save that predates background/tool_proficiencies/languages.
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("background");
        obj.remove("tool_proficiencies");
        obj.remove("languages");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.background, Background::Acolyte);
        assert!(loaded.tool_proficiencies.is_empty());
        assert!(loaded.languages.is_empty(), "legacy saves default to empty Vec<String>");
    }

    // ---- Creation for new SRD classes ----

    #[test]
    fn test_bard_gets_level_one_spell_slot() {
        let c = create_character("Vira".to_string(), Race::Human, Class::Bard, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![2]);
        assert_eq!(c.spell_slots_remaining, vec![2]);
        // CHA for our test_scores is 8 -> mod -1 -> min 1 inspiration.
        // Human +1 to all -> 9 -> mod -1 still. So expected 1.
        assert_eq!(c.class_features.bardic_inspiration_remaining, 1);
    }

    #[test]
    fn test_cleric_druid_sorcerer_get_slots() {
        let c = create_character("Prie".to_string(), Race::Human, Class::Cleric, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![2]);
        let c = create_character("Gaia".to_string(), Race::Human, Class::Druid, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![2]);
        let c = create_character("Sorc".to_string(), Race::Human, Class::Sorcerer, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![2]);
    }

    #[test]
    fn test_warlock_gets_single_slot() {
        let c = create_character("Patr".to_string(), Race::Human, Class::Warlock, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![1]);
        assert_eq!(c.spell_slots_remaining, vec![1]);
    }

    #[test]
    fn test_paladin_level_one_has_two_spell_slots() {
        // 2024 SRD: Paladin gains Spellcasting at level 1 with 2 first-level
        // spell slots. See docs/reference/paladin.md.
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        assert_eq!(c.spell_slots_max, vec![2]);
        assert_eq!(c.spell_slots_remaining, vec![2]);
    }

    #[test]
    fn test_ranger_level_one_has_no_slots() {
        // Ranger spellcasting is not in scope for issue #86 — retains the
        // half-caster table behavior (no slots at level 1).
        let c = create_character("Aran".to_string(), Race::Human, Class::Ranger, test_scores(), vec![]);
        assert!(c.spell_slots_max.is_empty());
        assert!(c.spell_slots_remaining.is_empty());
    }

    #[test]
    fn test_barbarian_starts_with_two_rage_uses() {
        let c = create_character("Krom".to_string(), Race::Human, Class::Barbarian, test_scores(), vec![]);
        assert_eq!(c.class_features.rage_uses_remaining, 2);
        assert!(!c.class_features.rage_active);
    }

    #[test]
    fn test_paladin_starts_with_lay_on_hands_pool() {
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        // 5 * paladin level = 5 at level 1
        assert_eq!(c.class_features.lay_on_hands_pool, 5);
    }

    #[test]
    fn test_monk_starts_with_no_ki() {
        // Monks unlock Ki at level 2 per SRD; level 1 pool is 0.
        let c = create_character("Pax".to_string(), Race::Human, Class::Monk, test_scores(), vec![]);
        assert_eq!(c.class_features.ki_points_remaining, 0);
    }

    #[test]
    fn test_cleric_level_one_has_no_channel_divinity() {
        let c = create_character("Prie".to_string(), Race::Human, Class::Cleric, test_scores(), vec![]);
        assert_eq!(c.class_features.channel_divinity_remaining, 0);
    }

    #[test]
    fn test_wizard_prepared_spells_mirror_known() {
        let c = create_character("Gan".to_string(), Race::Human, Class::Wizard, test_scores(), vec![]);
        assert_eq!(c.class_features.prepared_spells.len(), 4);
        for spell in &c.class_features.prepared_spells {
            assert!(c.known_spells.contains(spell), "prepared spell must be known: {}", spell);
        }
    }

    #[test]
    fn test_barbarian_hp_uses_d12() {
        // Fighter with same scores -> d10, so max_hp = 10 + con_mod (test_scores CON 13 + human +1 => 14 => +2 -> 12).
        // Barbarian d12 -> 12 + con_mod = 14.
        let c = create_character("Krom".to_string(), Race::Human, Class::Barbarian, test_scores(), vec![]);
        assert_eq!(c.max_hp, 14);
    }

    #[test]
    fn test_new_character_has_empty_attuned_items() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert!(c.attuned_items.is_empty());
    }

    #[test]
    fn test_character_missing_attuned_items_deserialize_defaults() {
        // Legacy save that predates attuned_items: the field missing from JSON
        // should deserialize to an empty Vec<ItemId> (via #[serde(default)]).
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("attuned_items");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert!(loaded.attuned_items.is_empty());
    }

    #[test]
    fn test_new_character_has_no_feats() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert_eq!(c.origin_feat, None);
        assert!(c.general_feats.is_empty());
    }

    #[test]
    fn test_new_character_defaults_to_unaligned() {
        use crate::types::Alignment;
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        // Newly created characters start Unaligned; the ChooseAlignment
        // creation step sets the value later in the wizard.
        assert_eq!(c.alignment, Alignment::Unaligned);
    }

    #[test]
    fn test_character_missing_alignment_deserializes_default() {
        use crate::types::Alignment;
        // Legacy save predating the alignment field: JSON without the
        // `alignment` key must deserialize to Alignment::Unaligned via
        // #[serde(default)].
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("alignment");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.alignment, Alignment::Unaligned);
    }

    #[test]
    fn test_character_missing_feat_fields_deserialize_defaults() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("origin_feat");
        obj.remove("general_feats");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.origin_feat, None);
        assert!(loaded.general_feats.is_empty());
    }

    #[test]
    fn test_new_character_has_no_subrace() {
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert_eq!(c.subrace, None);
    }

    #[test]
    fn test_character_missing_subrace_deserializes_none() {
        // Legacy save predating the subrace field: should default to None.
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("subrace");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.subrace, None);
    }

    #[test]
    fn test_character_subrace_roundtrips() {
        let mut c = create_character("Test".to_string(), Race::Elf, Class::Fighter, test_scores(), vec![]);
        c.subrace = Some("Wood Elf".to_string());
        let json = serde_json::to_string(&c).unwrap();
        let loaded: Character = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.subrace, Some("Wood Elf".to_string()));
    }

    #[test]
    fn test_initiative_bonus_from_feats_alert() {
        let mut c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        assert_eq!(initiative_bonus_from_feats(&c), 0, "no feats, no bonus");
        c.origin_feat = Some("Alert".to_string());
        assert_eq!(initiative_bonus_from_feats(&c), 5, "Alert grants +5");
    }

    #[test]
    fn test_initiative_bonus_from_feats_unknown_ignored() {
        let mut c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        c.origin_feat = Some("Nonexistent".to_string());
        c.general_feats = vec!["AlsoMissing".to_string()];
        assert_eq!(initiative_bonus_from_feats(&c), 0);
    }

    #[test]
    fn test_initiative_bonus_sums_origin_and_general() {
        let mut c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        c.origin_feat = Some("Alert".to_string());
        // No general feats grant initiative in our catalog, but verify summation
        // is non-destructive when there are no extra contributions.
        c.general_feats = vec!["Tough".to_string()];
        assert_eq!(initiative_bonus_from_feats(&c), 5);
    }

    #[test]
    fn test_character_missing_rest_fields_deserialize_defaults() {
        // Build a minimal legacy JSON missing hit_dice_remaining/class_features/exhaustion.
        // Round-trip create then strip the fields and assert defaults on load.
        let c = create_character("Test".to_string(), Race::Human, Class::Fighter, test_scores(), vec![]);
        let mut v: serde_json::Value = serde_json::to_value(&c).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.remove("hit_dice_remaining");
        obj.remove("class_features");
        obj.remove("exhaustion");
        let loaded: Character = serde_json::from_value(v).unwrap();
        assert_eq!(loaded.hit_dice_remaining, 0); // u32 default
        assert!(loaded.class_features.second_wind_available); // default_true kicks in
        assert!(loaded.class_features.action_surge_available);
        assert!(!loaded.class_features.arcane_recovery_used_today);
        assert_eq!(loaded.exhaustion, 0);
    }

    // ---- Expanded-catalog known-spell initialization ----

    #[test]
    fn test_wizard_default_known_spells_matches_mvp_catalog() {
        let c = create_character("Wizmage".to_string(), Race::Human, Class::Wizard, test_scores(), vec![]);
        assert_eq!(c.known_spells.len(), 8);
        assert!(c.known_spells.contains(&"Fire Bolt".to_string()));
        assert!(c.known_spells.contains(&"Shield".to_string()));
    }

    #[test]
    fn test_sorcerer_default_known_spells_populated() {
        let c = create_character("Sorc".to_string(), Race::Human, Class::Sorcerer, test_scores(), vec![]);
        assert!(!c.known_spells.is_empty(), "Sorcerer should start with known spells");
        // Must contain at least one cantrip and one leveled spell.
        let cantrips: Vec<&String> = c.known_spells.iter()
            .filter(|n| crate::spells::find_spell(n).map(|s| s.level == 0).unwrap_or(false))
            .collect();
        let leveled: Vec<&String> = c.known_spells.iter()
            .filter(|n| crate::spells::find_spell(n).map(|s| s.level >= 1).unwrap_or(false))
            .collect();
        assert!(!cantrips.is_empty(), "Sorcerer should know at least one cantrip");
        assert!(!leveled.is_empty(), "Sorcerer should know at least one leveled spell");
    }

    #[test]
    fn test_bard_default_known_spells_populated() {
        let c = create_character("Virtuoso".to_string(), Race::Human, Class::Bard, test_scores(), vec![]);
        assert!(!c.known_spells.is_empty());
        // Every Bard-known spell must be on the Bard spell list.
        for spell in &c.known_spells {
            let def = crate::spells::find_spell(spell).expect("known spell exists in catalog");
            assert!(def.is_class_spell("Bard"), "{} should be on the Bard list", spell);
        }
    }

    #[test]
    fn test_cleric_default_known_spells_populated_and_prepared() {
        let c = create_character("Priest".to_string(), Race::Human, Class::Cleric, test_scores(), vec![]);
        assert!(!c.known_spells.is_empty());
        // Prepared casters mirror known into prepared at creation.
        assert_eq!(c.class_features.prepared_spells, c.known_spells);
        for spell in &c.known_spells {
            let def = crate::spells::find_spell(spell).expect("known spell exists in catalog");
            assert!(def.is_class_spell("Cleric"), "{} should be on the Cleric list", spell);
        }
    }

    #[test]
    fn test_druid_default_known_spells_populated_and_prepared() {
        let c = create_character("Gaia".to_string(), Race::Human, Class::Druid, test_scores(), vec![]);
        assert!(!c.known_spells.is_empty());
        assert_eq!(c.class_features.prepared_spells, c.known_spells);
        for spell in &c.known_spells {
            let def = crate::spells::find_spell(spell).expect("known spell exists in catalog");
            assert!(def.is_class_spell("Druid"), "{} should be on the Druid list", spell);
        }
    }

    #[test]
    fn test_warlock_default_known_spells_populated() {
        let c = create_character("Patr".to_string(), Race::Human, Class::Warlock, test_scores(), vec![]);
        assert!(!c.known_spells.is_empty());
        for spell in &c.known_spells {
            let def = crate::spells::find_spell(spell).expect("known spell exists in catalog");
            assert!(def.is_class_spell("Warlock"), "{} should be on the Warlock list", spell);
        }
    }

    // Root-cause hypothesis (2026-04-16, issue #86):
    // Under 2024 SRD, Paladin gains Spellcasting at level 1 with 2 prepared
    // spells and 2 level-1 slots (see docs/reference/paladin.md). The engine
    // previously encoded 2014 SRD behavior (no slots until level 2). These
    // tests assert the 2024 behavior.
    #[test]
    fn test_paladin_level_one_has_starting_known_spells() {
        // 2024 SRD Paladin knows 2 level-1 spells at character creation.
        // The catalog offers Heroism and Bless (Searing Smite — SRD's
        // recommended pair — is not in the catalog).
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        assert_eq!(c.known_spells.len(), 2, "Paladin L1 should know 2 spells");
        assert!(c.known_spells.contains(&"Heroism".to_string()),
            "Paladin L1 known_spells should include Heroism: {:?}", c.known_spells);
        assert!(c.known_spells.contains(&"Bless".to_string()),
            "Paladin L1 known_spells should include Bless: {:?}", c.known_spells);
    }

    #[test]
    fn test_paladin_level_one_prepared_spells_mirror_known() {
        // Paladin is a prepared caster; prepared_spells should mirror
        // known_spells at character creation (same as Cleric/Druid/Wizard).
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        assert_eq!(c.class_features.prepared_spells, c.known_spells);
    }

    #[test]
    fn test_ranger_level_one_has_empty_known_spells() {
        // Ranger spellcasting is out of scope for issue #86 — it retains
        // the existing behavior (empty known_spells at level 1).
        let c = create_character("Aran".to_string(), Race::Human, Class::Ranger, test_scores(), vec![]);
        assert!(c.known_spells.is_empty());
    }

    #[test]
    fn test_non_casters_have_empty_known_spells() {
        for class in [Class::Barbarian, Class::Fighter, Class::Monk, Class::Rogue] {
            let c = create_character("T".to_string(), Race::Human, class, test_scores(), vec![]);
            assert!(c.known_spells.is_empty(), "{:?} should have no known spells", class);
        }
    }

    // ---- Weapon Mastery (feat/weapon-mastery) ----

    #[test]
    fn test_create_character_fighter_fills_one_mastery_slot_from_loadout() {
        // Fighter starting loadout has only a Longsword as a weapon (Shield
        // is armor and Chain Mail is armor). Fighter has 3 mastery slots
        // per SRD but only 1 weapon in the loadout, so we expect exactly 1
        // entry filled.
        let c = create_character(
            "Drizzt".to_string(), Race::Human, Class::Fighter, test_scores(), vec![],
        );
        assert_eq!(c.weapon_masteries, vec!["Longsword".to_string()]);
    }

    #[test]
    fn test_create_character_barbarian_fills_two_slots_from_loadout() {
        // Barbarian loadout: Greataxe + 4x Handaxe. Two slots, dedup so
        // Handaxe only fills once after Greataxe.
        let c = create_character(
            "Gorga".to_string(), Race::Human, Class::Barbarian, test_scores(), vec![],
        );
        assert_eq!(c.weapon_masteries.len(), 2);
        assert_eq!(c.weapon_masteries[0], "Greataxe");
        assert_eq!(c.weapon_masteries[1], "Handaxe");
    }

    #[test]
    fn test_create_character_paladin_fills_two_slots_from_loadout() {
        // Paladin loadout: Longsword + Shield + Chain Mail + 6x Javelin.
        // Shield is armor (no mastery), Longsword fills slot 1, Javelin
        // fills slot 2. Two slots per SRD.
        let c = create_character(
            "Alicia".to_string(), Race::Human, Class::Paladin, test_scores(), vec![],
        );
        assert_eq!(c.weapon_masteries.len(), 2);
        assert_eq!(c.weapon_masteries[0], "Longsword");
        assert_eq!(c.weapon_masteries[1], "Javelin");
    }

    #[test]
    fn test_create_character_ranger_fills_two_slots_from_loadout() {
        // Ranger loadout: Scimitar + Studded Leather + Shortsword + Longbow.
        // Slot 1: Scimitar (main hand). Slot 2: Shortsword (first extra).
        let c = create_character(
            "Ara".to_string(), Race::Human, Class::Ranger, test_scores(), vec![],
        );
        assert_eq!(c.weapon_masteries.len(), 2);
        assert_eq!(c.weapon_masteries[0], "Scimitar");
        assert_eq!(c.weapon_masteries[1], "Shortsword");
    }

    #[test]
    fn test_create_character_non_mastery_class_has_empty_masteries() {
        // Wizard: no mastery slots. Weapons in loadout are ignored.
        let c = create_character(
            "Merlin".to_string(), Race::Human, Class::Wizard, test_scores(), vec![],
        );
        assert!(c.weapon_masteries.is_empty());
        // Bard: no mastery slots either.
        let c = create_character(
            "Lute".to_string(), Race::Human, Class::Bard, test_scores(), vec![],
        );
        assert!(c.weapon_masteries.is_empty());
    }

    #[test]
    fn test_character_weapon_masteries_legacy_save_defaults_to_empty() {
        // A save written before weapon_masteries existed should deserialize
        // with an empty vec via #[serde(default)].
        let c = create_character(
            "Drizzt".to_string(), Race::Human, Class::Fighter, test_scores(), vec![],
        );
        let mut json: serde_json::Value = serde_json::to_value(&c).unwrap();
        // Remove the field as if the save predates it.
        json.as_object_mut().unwrap().remove("weapon_masteries");
        let loaded: Character = serde_json::from_value(json).unwrap();
        assert!(loaded.weapon_masteries.is_empty());
    }

    // ---- Ability score cap (SRD 5.1: scores cannot exceed 20) ----

    #[test]
    fn test_ability_score_cap_clamps_scores_exceeding_20() {
        // Start with scores that would exceed 20 after racial bonus.
        // Elf gives +2 DEX; if we set DEX to 19, post-bonus would be 21 -> clamped to 20.
        let mut scores = test_scores();
        scores.insert(Ability::Dexterity, 19); // 19 + 2 (Elf) = 21, should clamp to 20
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, scores, vec![]);
        assert_eq!(c.ability_scores[&Ability::Dexterity], 20,
            "DEX 19 + Elf +2 should be clamped to 20, not 21");
    }

    #[test]
    fn test_ability_score_cap_does_not_clamp_scores_below_20() {
        // Scores that don't exceed 20 should be unchanged.
        let mut scores = test_scores(); // STR 15, DEX 14, ...
        scores.insert(Ability::Dexterity, 14); // 14 + 2 (Elf) = 16, no clamp needed
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, scores, vec![]);
        assert_eq!(c.ability_scores[&Ability::Dexterity], 16,
            "DEX 14 + Elf +2 = 16, should not be clamped");
    }

    #[test]
    fn test_ability_score_cap_allows_exactly_20() {
        // A score of exactly 20 must not be reduced.
        let mut scores = test_scores();
        scores.insert(Ability::Dexterity, 18); // 18 + 2 (Elf) = 20, exactly at cap
        let c = create_character("Test".to_string(), Race::Elf, Class::Rogue, scores, vec![]);
        assert_eq!(c.ability_scores[&Ability::Dexterity], 20,
            "DEX 18 + Elf +2 = 20 exactly, must not be reduced");
    }
}
