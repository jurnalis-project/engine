// jurnalis-engine/src/character/class.rs
use serde::{Deserialize, Serialize};
use crate::types::{Ability, Skill};

/// Per-class feature-use tracking. All fields default so older saves
/// deserialize cleanly.
///
/// Short-rest features (refresh on short OR long rest):
///   - `second_wind_available` (Fighter)
///   - `channel_divinity_remaining` (Cleric, Paladin)
///   - `ki_points_remaining` (Monk)
///   - Warlock Pact-Magic spell slots (handled via `spell_slots_remaining`)
///   - `rage_uses_remaining` (Barbarian: 1 use per short rest, all on long)
///
/// Long-rest features (refresh only on long rest):
///   - `action_surge_available` (Fighter)
///   - `arcane_recovery_used_today` (Wizard — true = already used today)
///   - `bardic_inspiration_remaining` (Bard, 2024 SRD refreshes on long rest)
///   - `lay_on_hands_pool` (Paladin)
///
/// Turn-scoped flags (reset at start of player turn, not by resting):
///   - `cunning_action_used` (Rogue — used a bonus action this turn)
///   - `sneak_attack_used_this_turn` (Rogue — Sneak Attack once per turn cap)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassFeatureState {
    // ---- Fighter ----
    #[serde(default = "default_true")]
    pub second_wind_available: bool,
    #[serde(default = "default_true")]
    pub action_surge_available: bool,

    // ---- Wizard ----
    #[serde(default)]
    pub arcane_recovery_used_today: bool,
    /// Wizard spell-prep list (MVP: mirrors `known_spells`).
    #[serde(default)]
    pub prepared_spells: Vec<String>,

    // ---- Barbarian ----
    #[serde(default)]
    pub rage_uses_remaining: u32,
    #[serde(default)]
    pub rage_active: bool,

    // ---- Bard ----
    #[serde(default)]
    pub bardic_inspiration_remaining: u32,

    // ---- Cleric / Paladin ----
    #[serde(default)]
    pub channel_divinity_remaining: u32,
    #[serde(default)]
    pub lay_on_hands_pool: u32,

    // ---- Monk ----
    #[serde(default)]
    pub ki_points_remaining: u32,

    // ---- Rogue (turn-scoped flags) ----
    #[serde(default)]
    pub cunning_action_used: bool,
    #[serde(default)]
    pub sneak_attack_used_this_turn: bool,

    // ---- Concentration (spells) ----
    /// Name of the spell the character is currently concentrating on, or
    /// `None`. Starting a new concentration spell drops the previous one
    /// (per SRD 5.1: a caster can only concentrate on one spell at a time).
    /// Also cleared when the caster fails a concentration save or drops
    /// the spell deliberately.
    #[serde(default)]
    pub concentration_spell: Option<String>,
}

fn default_true() -> bool { true }

impl Default for ClassFeatureState {
    fn default() -> Self {
        Self {
            second_wind_available: true,
            action_surge_available: true,
            arcane_recovery_used_today: false,
            prepared_spells: Vec::new(),
            rage_uses_remaining: 0,
            rage_active: false,
            bardic_inspiration_remaining: 0,
            channel_divinity_remaining: 0,
            lay_on_hands_pool: 0,
            ki_points_remaining: 0,
            cunning_action_used: false,
            sneak_attack_used_this_turn: false,
            concentration_spell: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Class {
    Barbarian,
    Bard,
    Cleric,
    Druid,
    Fighter,
    Monk,
    Paladin,
    Ranger,
    Rogue,
    Sorcerer,
    Warlock,
    Wizard,
}

impl Class {
    pub fn all() -> &'static [Class] {
        &[
            Class::Barbarian,
            Class::Bard,
            Class::Cleric,
            Class::Druid,
            Class::Fighter,
            Class::Monk,
            Class::Paladin,
            Class::Ranger,
            Class::Rogue,
            Class::Sorcerer,
            Class::Warlock,
            Class::Wizard,
        ]
    }

    pub fn hit_die(&self) -> u32 {
        match self {
            Class::Barbarian => 12,
            Class::Bard | Class::Cleric | Class::Druid | Class::Monk
            | Class::Rogue | Class::Warlock => 8,
            Class::Fighter | Class::Paladin | Class::Ranger => 10,
            Class::Sorcerer | Class::Wizard => 6,
        }
    }

    pub fn saving_throw_proficiencies(&self) -> Vec<Ability> {
        match self {
            Class::Barbarian => vec![Ability::Strength, Ability::Constitution],
            Class::Bard => vec![Ability::Dexterity, Ability::Charisma],
            Class::Cleric => vec![Ability::Wisdom, Ability::Charisma],
            Class::Druid => vec![Ability::Intelligence, Ability::Wisdom],
            Class::Fighter => vec![Ability::Strength, Ability::Constitution],
            Class::Monk => vec![Ability::Strength, Ability::Dexterity],
            Class::Paladin => vec![Ability::Wisdom, Ability::Charisma],
            Class::Ranger => vec![Ability::Strength, Ability::Dexterity],
            Class::Rogue => vec![Ability::Dexterity, Ability::Intelligence],
            Class::Sorcerer => vec![Ability::Constitution, Ability::Charisma],
            Class::Warlock => vec![Ability::Wisdom, Ability::Charisma],
            Class::Wizard => vec![Ability::Intelligence, Ability::Wisdom],
        }
    }

    pub fn skill_choices(&self) -> Vec<Skill> {
        match self {
            Class::Barbarian => vec![
                Skill::AnimalHandling, Skill::Athletics, Skill::Intimidation,
                Skill::Nature, Skill::Perception, Skill::Survival,
            ],
            // Bards choose from any skill per SRD 2024.
            Class::Bard => Skill::all().to_vec(),
            Class::Cleric => vec![
                Skill::History, Skill::Insight, Skill::Medicine,
                Skill::Persuasion, Skill::Religion,
            ],
            Class::Druid => vec![
                Skill::AnimalHandling, Skill::Arcana, Skill::Insight,
                Skill::Medicine, Skill::Nature, Skill::Perception,
                Skill::Religion, Skill::Survival,
            ],
            Class::Fighter => vec![
                Skill::Acrobatics, Skill::AnimalHandling, Skill::Athletics,
                Skill::History, Skill::Insight, Skill::Intimidation,
                Skill::Perception, Skill::Survival,
            ],
            Class::Monk => vec![
                Skill::Acrobatics, Skill::Athletics, Skill::History,
                Skill::Insight, Skill::Religion, Skill::Stealth,
            ],
            Class::Paladin => vec![
                Skill::Athletics, Skill::Insight, Skill::Intimidation,
                Skill::Medicine, Skill::Persuasion, Skill::Religion,
            ],
            Class::Ranger => vec![
                Skill::AnimalHandling, Skill::Athletics, Skill::Insight,
                Skill::Investigation, Skill::Nature, Skill::Perception,
                Skill::Stealth, Skill::Survival,
            ],
            Class::Rogue => vec![
                Skill::Acrobatics, Skill::Athletics, Skill::Deception,
                Skill::Insight, Skill::Intimidation, Skill::Investigation,
                Skill::Perception, Skill::Performance, Skill::Persuasion,
                Skill::SleightOfHand, Skill::Stealth,
            ],
            Class::Sorcerer => vec![
                Skill::Arcana, Skill::Deception, Skill::Insight,
                Skill::Intimidation, Skill::Persuasion, Skill::Religion,
            ],
            Class::Warlock => vec![
                Skill::Arcana, Skill::Deception, Skill::History,
                Skill::Intimidation, Skill::Investigation, Skill::Nature,
                Skill::Religion,
            ],
            Class::Wizard => vec![
                Skill::Arcana, Skill::History, Skill::Insight,
                Skill::Investigation, Skill::Medicine, Skill::Religion,
            ],
        }
    }

    pub fn skill_choice_count(&self) -> usize {
        match self {
            Class::Bard | Class::Ranger => 3,
            Class::Rogue => 4,
            _ => 2,
        }
    }

    pub fn proficiency_bonus(level: u32) -> i32 {
        match level { 1..=4 => 2, 5..=8 => 3, 9..=12 => 4, 13..=16 => 5, 17..=20 => 6, _ => 2 }
    }

    /// Number of Weapon Mastery slots unlocked at level 1 per 2024 SRD.
    /// Mastery-unlocking classes (Fighter 3, Barbarian/Paladin/Ranger 2)
    /// can pre-populate their `Character.weapon_masteries` from their
    /// starting loadout. Non-mastery classes return 0 — the Weapon Master
    /// feat (#28) is the only future path for them to unlock mastery.
    /// See `docs/specs/weapon-mastery.md`.
    pub fn starting_weapon_masteries(&self) -> u32 {
        match self {
            Class::Fighter => 3,
            Class::Barbarian | Class::Paladin | Class::Ranger => 2,
            _ => 0,
        }
    }

    /// Level-1 spell-slot vector for this class. Returns an empty vector for
    /// non-casters and for classes that unlock spellcasting after level 1
    /// (Paladin, Ranger).
    pub fn starting_spell_slots(&self) -> Vec<i32> {
        match self {
            Class::Bard | Class::Cleric | Class::Druid
            | Class::Sorcerer | Class::Wizard => vec![2],
            // Warlock Pact Magic: 1 slot at level 1.
            Class::Warlock => vec![1],
            // Half-casters start at level 2; no slots at level 1.
            Class::Paladin | Class::Ranger => Vec::new(),
            // Pure martials
            Class::Barbarian | Class::Fighter | Class::Monk | Class::Rogue => Vec::new(),
        }
    }
}

/// Describes the starting equipment loadout for a class.
/// Contains item names (matching SRD const table entries) categorized by slot.
pub struct StartingLoadout {
    pub main_hand: Option<&'static str>,
    pub off_hand: Option<&'static str>,
    pub body: Option<&'static str>,
    pub extra_inventory: &'static [&'static str],
}

impl Class {
    pub fn starting_loadout(&self) -> StartingLoadout {
        match self {
            Class::Barbarian => StartingLoadout {
                main_hand: Some("Greataxe"),
                off_hand: None,
                body: None,
                extra_inventory: &["Handaxe", "Handaxe", "Handaxe", "Handaxe"],
            },
            Class::Bard => StartingLoadout {
                main_hand: Some("Dagger"),
                off_hand: None,
                body: Some("Leather"),
                extra_inventory: &["Dagger"],
            },
            Class::Cleric => StartingLoadout {
                main_hand: Some("Mace"),
                off_hand: Some("Shield"),
                body: Some("Chain Shirt"),
                extra_inventory: &[],
            },
            Class::Druid => StartingLoadout {
                main_hand: Some("Sickle"),
                off_hand: Some("Shield"),
                body: Some("Leather"),
                extra_inventory: &["Quarterstaff"],
            },
            Class::Fighter => StartingLoadout {
                main_hand: Some("Longsword"),
                off_hand: Some("Shield"),
                body: Some("Chain Mail"),
                extra_inventory: &[],
            },
            Class::Monk => StartingLoadout {
                main_hand: Some("Spear"),
                off_hand: None,
                body: None,
                extra_inventory: &["Dagger", "Dagger", "Dagger", "Dagger", "Dagger"],
            },
            Class::Paladin => StartingLoadout {
                main_hand: Some("Longsword"),
                off_hand: Some("Shield"),
                body: Some("Chain Mail"),
                extra_inventory: &["Javelin", "Javelin", "Javelin", "Javelin", "Javelin", "Javelin"],
            },
            Class::Ranger => StartingLoadout {
                main_hand: Some("Scimitar"),
                off_hand: None,
                body: Some("Studded Leather"),
                extra_inventory: &["Shortsword", "Longbow"],
            },
            Class::Rogue => StartingLoadout {
                main_hand: Some("Shortsword"),
                off_hand: None,
                body: Some("Leather"),
                extra_inventory: &["Dagger"],
            },
            Class::Sorcerer => StartingLoadout {
                main_hand: Some("Spear"),
                off_hand: None,
                body: None,
                extra_inventory: &["Dagger", "Dagger"],
            },
            Class::Warlock => StartingLoadout {
                main_hand: Some("Sickle"),
                off_hand: None,
                body: Some("Leather"),
                extra_inventory: &["Dagger", "Dagger"],
            },
            Class::Wizard => StartingLoadout {
                main_hand: Some("Quarterstaff"),
                off_hand: None,
                body: None,
                extra_inventory: &["Dagger"],
            },
        }
    }
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Class::Barbarian => write!(f, "Barbarian"),
            Class::Bard => write!(f, "Bard"),
            Class::Cleric => write!(f, "Cleric"),
            Class::Druid => write!(f, "Druid"),
            Class::Fighter => write!(f, "Fighter"),
            Class::Monk => write!(f, "Monk"),
            Class::Paladin => write!(f, "Paladin"),
            Class::Ranger => write!(f, "Ranger"),
            Class::Rogue => write!(f, "Rogue"),
            Class::Sorcerer => write!(f, "Sorcerer"),
            Class::Warlock => write!(f, "Warlock"),
            Class::Wizard => write!(f, "Wizard"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn test_fighter_hit_die() { assert_eq!(Class::Fighter.hit_die(), 10); }
    #[test] fn test_rogue_gets_4_skills() { assert_eq!(Class::Rogue.skill_choice_count(), 4); }
    #[test]
    fn test_proficiency_bonus_scaling() {
        assert_eq!(Class::proficiency_bonus(1), 2);
        assert_eq!(Class::proficiency_bonus(5), 3);
        assert_eq!(Class::proficiency_bonus(9), 4);
        assert_eq!(Class::proficiency_bonus(13), 5);
        assert_eq!(Class::proficiency_bonus(17), 6);
    }
    #[test]
    fn test_wizard_saves() {
        let saves = Class::Wizard.saving_throw_proficiencies();
        assert!(saves.contains(&Ability::Intelligence));
        assert!(saves.contains(&Ability::Wisdom));
    }

    #[test]
    fn test_fighter_starting_loadout() {
        let loadout = Class::Fighter.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Longsword"));
        assert_eq!(loadout.off_hand, Some("Shield"));
        assert_eq!(loadout.body, Some("Chain Mail"));
        assert!(loadout.extra_inventory.is_empty());
    }

    #[test]
    fn test_rogue_starting_loadout() {
        let loadout = Class::Rogue.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Shortsword"));
        assert_eq!(loadout.off_hand, None);
        assert_eq!(loadout.body, Some("Leather"));
        assert_eq!(loadout.extra_inventory, &["Dagger"]);
    }

    #[test]
    fn test_class_feature_state_defaults() {
        let f = ClassFeatureState::default();
        assert!(f.second_wind_available);
        assert!(f.action_surge_available);
        assert!(!f.arcane_recovery_used_today);
    }

    #[test]
    fn test_class_feature_state_missing_fields_deserialize_defaults() {
        // Simulate a save from before the field existed: use default().
        let json = "{}";
        let parsed: ClassFeatureState = serde_json::from_str(json).unwrap();
        assert!(parsed.second_wind_available);
        assert!(parsed.action_surge_available);
        assert!(!parsed.arcane_recovery_used_today);
    }

    #[test]
    fn test_wizard_starting_loadout() {
        let loadout = Class::Wizard.starting_loadout();
        assert_eq!(loadout.main_hand, Some("Quarterstaff"));
        assert_eq!(loadout.off_hand, None);
        assert_eq!(loadout.body, None);
        assert_eq!(loadout.extra_inventory, &["Dagger"]);
    }

    // ---- SRD full-class expansion (feat/remaining-srd-classes) ----

    #[test]
    fn test_all_includes_twelve_classes() {
        let all = Class::all();
        assert_eq!(all.len(), 12);
        assert!(all.contains(&Class::Barbarian));
        assert!(all.contains(&Class::Bard));
        assert!(all.contains(&Class::Cleric));
        assert!(all.contains(&Class::Druid));
        assert!(all.contains(&Class::Fighter));
        assert!(all.contains(&Class::Monk));
        assert!(all.contains(&Class::Paladin));
        assert!(all.contains(&Class::Ranger));
        assert!(all.contains(&Class::Rogue));
        assert!(all.contains(&Class::Sorcerer));
        assert!(all.contains(&Class::Warlock));
        assert!(all.contains(&Class::Wizard));
    }

    #[test]
    fn test_hit_dice_per_class() {
        assert_eq!(Class::Barbarian.hit_die(), 12);
        assert_eq!(Class::Bard.hit_die(), 8);
        assert_eq!(Class::Cleric.hit_die(), 8);
        assert_eq!(Class::Druid.hit_die(), 8);
        assert_eq!(Class::Fighter.hit_die(), 10);
        assert_eq!(Class::Monk.hit_die(), 8);
        assert_eq!(Class::Paladin.hit_die(), 10);
        assert_eq!(Class::Ranger.hit_die(), 10);
        assert_eq!(Class::Rogue.hit_die(), 8);
        assert_eq!(Class::Sorcerer.hit_die(), 6);
        assert_eq!(Class::Warlock.hit_die(), 8);
        assert_eq!(Class::Wizard.hit_die(), 6);
    }

    #[test]
    fn test_saving_throw_proficiencies_per_class() {
        // Exactly two proficiencies per class per SRD.
        for class in Class::all() {
            let saves = class.saving_throw_proficiencies();
            assert_eq!(saves.len(), 2, "{:?} should have 2 saves, got {:?}", class, saves);
        }
        // Spot-check a few.
        let bard = Class::Bard.saving_throw_proficiencies();
        assert!(bard.contains(&Ability::Dexterity));
        assert!(bard.contains(&Ability::Charisma));

        let barb = Class::Barbarian.saving_throw_proficiencies();
        assert!(barb.contains(&Ability::Strength));
        assert!(barb.contains(&Ability::Constitution));

        let monk = Class::Monk.saving_throw_proficiencies();
        assert!(monk.contains(&Ability::Strength));
        assert!(monk.contains(&Ability::Dexterity));

        let paladin = Class::Paladin.saving_throw_proficiencies();
        assert!(paladin.contains(&Ability::Wisdom));
        assert!(paladin.contains(&Ability::Charisma));

        let sorc = Class::Sorcerer.saving_throw_proficiencies();
        assert!(sorc.contains(&Ability::Constitution));
        assert!(sorc.contains(&Ability::Charisma));

        let warlock = Class::Warlock.saving_throw_proficiencies();
        assert!(warlock.contains(&Ability::Wisdom));
        assert!(warlock.contains(&Ability::Charisma));
    }

    #[test]
    fn test_skill_choice_counts_per_class() {
        assert_eq!(Class::Barbarian.skill_choice_count(), 2);
        assert_eq!(Class::Bard.skill_choice_count(), 3);
        assert_eq!(Class::Cleric.skill_choice_count(), 2);
        assert_eq!(Class::Druid.skill_choice_count(), 2);
        assert_eq!(Class::Fighter.skill_choice_count(), 2);
        assert_eq!(Class::Monk.skill_choice_count(), 2);
        assert_eq!(Class::Paladin.skill_choice_count(), 2);
        assert_eq!(Class::Ranger.skill_choice_count(), 3);
        assert_eq!(Class::Rogue.skill_choice_count(), 4);
        assert_eq!(Class::Sorcerer.skill_choice_count(), 2);
        assert_eq!(Class::Warlock.skill_choice_count(), 2);
        assert_eq!(Class::Wizard.skill_choice_count(), 2);
    }

    #[test]
    fn test_skill_choices_include_expected_entries() {
        // Barbarian: Survival is an option.
        assert!(Class::Barbarian.skill_choices().contains(&Skill::Survival));
        // Cleric: Religion is an option.
        assert!(Class::Cleric.skill_choices().contains(&Skill::Religion));
        // Druid: Nature is an option.
        assert!(Class::Druid.skill_choices().contains(&Skill::Nature));
        // Monk: Stealth is an option.
        assert!(Class::Monk.skill_choices().contains(&Skill::Stealth));
        // Paladin: Persuasion is an option.
        assert!(Class::Paladin.skill_choices().contains(&Skill::Persuasion));
        // Ranger: Investigation is an option.
        assert!(Class::Ranger.skill_choices().contains(&Skill::Investigation));
        // Sorcerer: Arcana is an option.
        assert!(Class::Sorcerer.skill_choices().contains(&Skill::Arcana));
        // Warlock: Deception is an option.
        assert!(Class::Warlock.skill_choices().contains(&Skill::Deception));
    }

    #[test]
    fn test_bard_can_pick_any_skill() {
        // Bards choose any 3 skills (see SRD). Expose the full Skill list.
        let choices = Class::Bard.skill_choices();
        assert_eq!(choices.len(), Skill::all().len());
        for skill in Skill::all() {
            assert!(choices.contains(skill), "Bard should be able to pick {:?}", skill);
        }
    }

    #[test]
    fn test_starting_loadouts_for_new_classes() {
        let barb = Class::Barbarian.starting_loadout();
        assert_eq!(barb.main_hand, Some("Greataxe"));
        assert!(barb.extra_inventory.contains(&"Handaxe"));

        let bard = Class::Bard.starting_loadout();
        assert_eq!(bard.main_hand, Some("Dagger"));
        assert_eq!(bard.body, Some("Leather"));

        let cleric = Class::Cleric.starting_loadout();
        assert_eq!(cleric.main_hand, Some("Mace"));
        assert_eq!(cleric.off_hand, Some("Shield"));
        assert_eq!(cleric.body, Some("Chain Shirt"));

        let druid = Class::Druid.starting_loadout();
        assert_eq!(druid.main_hand, Some("Sickle"));
        assert_eq!(druid.off_hand, Some("Shield"));
        assert_eq!(druid.body, Some("Leather"));

        let monk = Class::Monk.starting_loadout();
        assert_eq!(monk.main_hand, Some("Spear"));
        assert!(monk.extra_inventory.contains(&"Dagger"));

        let paladin = Class::Paladin.starting_loadout();
        assert_eq!(paladin.main_hand, Some("Longsword"));
        assert_eq!(paladin.off_hand, Some("Shield"));
        assert_eq!(paladin.body, Some("Chain Mail"));
        assert!(paladin.extra_inventory.contains(&"Javelin"));

        let ranger = Class::Ranger.starting_loadout();
        assert_eq!(ranger.main_hand, Some("Scimitar"));
        assert_eq!(ranger.body, Some("Studded Leather"));
        assert!(ranger.extra_inventory.contains(&"Longbow"));
        assert!(ranger.extra_inventory.contains(&"Shortsword"));

        let sorc = Class::Sorcerer.starting_loadout();
        assert_eq!(sorc.main_hand, Some("Spear"));
        assert!(sorc.extra_inventory.contains(&"Dagger"));

        let warlock = Class::Warlock.starting_loadout();
        assert_eq!(warlock.main_hand, Some("Sickle"));
        assert_eq!(warlock.body, Some("Leather"));
        assert!(warlock.extra_inventory.contains(&"Dagger"));
    }

    #[test]
    fn test_starting_loadout_names_resolve_to_srd_tables() {
        use crate::equipment::{SRD_WEAPONS, SRD_ARMOR};
        for class in Class::all() {
            let loadout = class.starting_loadout();
            for candidate in [loadout.main_hand, loadout.off_hand, loadout.body].into_iter().flatten() {
                let in_weapons = SRD_WEAPONS.iter().any(|w| w.name == candidate);
                let in_armor = SRD_ARMOR.iter().any(|a| a.name == candidate);
                assert!(
                    in_weapons || in_armor,
                    "{:?} loadout item '{}' not found in SRD tables",
                    class, candidate,
                );
            }
            for name in loadout.extra_inventory {
                let in_weapons = SRD_WEAPONS.iter().any(|w| w.name == *name);
                let in_armor = SRD_ARMOR.iter().any(|a| a.name == *name);
                assert!(
                    in_weapons || in_armor,
                    "{:?} extra inventory '{}' not found in SRD tables",
                    class, name,
                );
            }
        }
    }

    #[test]
    fn test_class_display_for_new_classes() {
        assert_eq!(Class::Barbarian.to_string(), "Barbarian");
        assert_eq!(Class::Bard.to_string(), "Bard");
        assert_eq!(Class::Cleric.to_string(), "Cleric");
        assert_eq!(Class::Druid.to_string(), "Druid");
        assert_eq!(Class::Monk.to_string(), "Monk");
        assert_eq!(Class::Paladin.to_string(), "Paladin");
        assert_eq!(Class::Ranger.to_string(), "Ranger");
        assert_eq!(Class::Sorcerer.to_string(), "Sorcerer");
        assert_eq!(Class::Warlock.to_string(), "Warlock");
    }

    // ---- Level-1 spell-slot initialization per class ----

    #[test]
    fn test_starting_spell_slots_for_casters() {
        assert_eq!(Class::Bard.starting_spell_slots(), vec![2]);
        assert_eq!(Class::Cleric.starting_spell_slots(), vec![2]);
        assert_eq!(Class::Druid.starting_spell_slots(), vec![2]);
        assert_eq!(Class::Sorcerer.starting_spell_slots(), vec![2]);
        assert_eq!(Class::Warlock.starting_spell_slots(), vec![1]);
        assert_eq!(Class::Wizard.starting_spell_slots(), vec![2]);
    }

    #[test]
    fn test_starting_spell_slots_empty_for_non_casters_and_late_casters() {
        // Non-casters
        assert!(Class::Barbarian.starting_spell_slots().is_empty());
        assert!(Class::Fighter.starting_spell_slots().is_empty());
        assert!(Class::Monk.starting_spell_slots().is_empty());
        assert!(Class::Rogue.starting_spell_slots().is_empty());
        // Classes whose spellcasting unlocks at level 2
        assert!(Class::Paladin.starting_spell_slots().is_empty());
        assert!(Class::Ranger.starting_spell_slots().is_empty());
    }

    // ---- ClassFeatureState additions ----

    #[test]
    fn test_class_feature_state_has_new_fields_with_defaults() {
        let f = ClassFeatureState::default();
        assert_eq!(f.rage_uses_remaining, 0);
        assert!(!f.rage_active);
        assert_eq!(f.bardic_inspiration_remaining, 0);
        assert_eq!(f.channel_divinity_remaining, 0);
        assert_eq!(f.lay_on_hands_pool, 0);
        assert_eq!(f.ki_points_remaining, 0);
        assert!(!f.cunning_action_used);
        assert!(!f.sneak_attack_used_this_turn);
        assert!(f.prepared_spells.is_empty());
        // New in feat/expanded-spell-catalog: concentration tracking.
        assert!(f.concentration_spell.is_none());
    }

    // ---- Weapon Mastery (feat/weapon-mastery) ----

    #[test]
    fn test_starting_weapon_masteries_for_mastery_classes() {
        // 2024 SRD: Fighter gets 3, Barbarian/Paladin/Ranger get 2.
        assert_eq!(Class::Fighter.starting_weapon_masteries(), 3);
        assert_eq!(Class::Barbarian.starting_weapon_masteries(), 2);
        assert_eq!(Class::Paladin.starting_weapon_masteries(), 2);
        assert_eq!(Class::Ranger.starting_weapon_masteries(), 2);
    }

    #[test]
    fn test_starting_weapon_masteries_zero_for_non_mastery_classes() {
        // 2024 SRD: only Fighter/Barbarian/Paladin/Ranger unlock mastery
        // at level 1. Everyone else relies on the Weapon Master feat,
        // which is tracked separately by issue #28.
        assert_eq!(Class::Bard.starting_weapon_masteries(), 0);
        assert_eq!(Class::Cleric.starting_weapon_masteries(), 0);
        assert_eq!(Class::Druid.starting_weapon_masteries(), 0);
        assert_eq!(Class::Monk.starting_weapon_masteries(), 0);
        assert_eq!(Class::Rogue.starting_weapon_masteries(), 0);
        assert_eq!(Class::Sorcerer.starting_weapon_masteries(), 0);
        assert_eq!(Class::Warlock.starting_weapon_masteries(), 0);
        assert_eq!(Class::Wizard.starting_weapon_masteries(), 0);
    }

    #[test]
    fn test_class_feature_state_legacy_json_deserializes_with_defaults() {
        // Previous field set; none of the new fields included.
        let legacy_json = r#"{
            "second_wind_available": true,
            "action_surge_available": true,
            "arcane_recovery_used_today": false
        }"#;
        let loaded: ClassFeatureState = serde_json::from_str(legacy_json).unwrap();
        assert!(loaded.second_wind_available);
        assert!(loaded.action_surge_available);
        assert!(!loaded.arcane_recovery_used_today);
        // New fields default.
        assert_eq!(loaded.rage_uses_remaining, 0);
        assert!(!loaded.rage_active);
        assert_eq!(loaded.bardic_inspiration_remaining, 0);
        assert_eq!(loaded.channel_divinity_remaining, 0);
        assert_eq!(loaded.lay_on_hands_pool, 0);
        assert_eq!(loaded.ki_points_remaining, 0);
        assert!(!loaded.cunning_action_used);
        assert!(!loaded.sneak_attack_used_this_turn);
        assert!(loaded.prepared_spells.is_empty());
        // Legacy saves have no concentration_spell; field defaults to None.
        assert!(loaded.concentration_spell.is_none());
    }
}
