// jurnalis-engine/src/character/mod.rs
pub mod race;
pub mod class;
pub mod background;
pub mod feat;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use rand::Rng;
use crate::types::{Ability, Skill, ItemId};
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
        tool_proficiencies: Vec::new(),
        languages: vec!["Common".to_string()],
        attuned_items: Vec::new(),
        origin_feat: None,
        general_feats: Vec::new(),
    }
}

/// Starting known-spell list per caster class at level 1.
///
/// - Wizard retains the MVP canonical list for save back-compat (6 spells).
/// - Other full casters (Bard/Cleric/Druid/Sorcerer) and Warlock get a
///   small, thematic level-1 list drawn from their class spell list.
/// - Prepared casters (Cleric/Druid/Paladin/Wizard) receive this list as
///   the default prepared set; Paladin/Ranger still start with empty
///   prepared lists since they lack slots at level 1 per SRD.
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
        // Half-casters: level 1 has no slots; they lack known spells until level 2.
        Class::Paladin | Class::Ranger => Vec::new(),
        Class::Barbarian | Class::Fighter | Class::Monk | Class::Rogue => Vec::new(),
    }
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
            features.prepared_spells = known_spells.to_vec();
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
        // Wizard knows all 6 MVP spells
        assert_eq!(c.known_spells.len(), 6);
        assert!(c.known_spells.contains(&"Fire Bolt".to_string()));
        assert!(c.known_spells.contains(&"Prestidigitation".to_string()));
        assert!(c.known_spells.contains(&"Magic Missile".to_string()));
        assert!(c.known_spells.contains(&"Burning Hands".to_string()));
        assert!(c.known_spells.contains(&"Sleep".to_string()));
        assert!(c.known_spells.contains(&"Shield".to_string()));
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
    fn test_paladin_ranger_no_slots_at_level_one() {
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        assert!(c.spell_slots_max.is_empty());
        assert!(c.spell_slots_remaining.is_empty());
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
        assert_eq!(c.class_features.prepared_spells, c.known_spells);
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
        assert_eq!(c.known_spells.len(), 6);
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

    #[test]
    fn test_paladin_level_one_has_empty_known_spells() {
        // Paladin/Ranger unlock spellcasting at level 2 per SRD; level 1
        // starts with an empty list.
        let c = create_character("Pally".to_string(), Race::Human, Class::Paladin, test_scores(), vec![]);
        assert!(c.known_spells.is_empty());
        // prepared_spells also empty for Paladin at level 1.
        assert!(c.class_features.prepared_spells.is_empty());
    }

    #[test]
    fn test_ranger_level_one_has_empty_known_spells() {
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
}
