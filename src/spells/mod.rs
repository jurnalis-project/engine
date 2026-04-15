// jurnalis-engine/src/spells/mod.rs
// Spell definitions, slot tracking, and casting resolution.
// Dependencies: types.rs, state/ only (no feature module imports).

use rand::Rng;
use serde::{Deserialize, Serialize};
use crate::types::Ability;
use crate::rules::dice::{roll_d20, roll_dice};

/// Identifies a spell in the system.
///
/// The `classes` field uses lowercase class-name strings (e.g. `"wizard"`,
/// `"cleric"`) to avoid a cross-module import of the `Class` enum from
/// `character/`. Membership queries go through [`SpellDef::is_class_spell`].
/// The `ritual` and `concentration` flags follow the SRD 5.1 tags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellDef {
    pub name: &'static str,
    pub level: u32,          // 0 = cantrip
    pub school: SpellSchool,
    pub casting: CastingMode,
    /// Requires concentration per SRD 5.1. Casting a new concentration spell
    /// drops any previous one.
    pub concentration: bool,
    /// Has the Ritual tag per SRD 5.1. Can be cast as a ritual (no slot
    /// consumed) using `cast <spell> ritual` / `cast <spell> as ritual`.
    pub ritual: bool,
    /// Lowercase class-name strings (e.g. `"wizard"`). Used for per-class
    /// spell-list population.
    pub classes: &'static [&'static str],
}

impl SpellDef {
    /// True when this spell appears on the given class's spell list.
    /// The `class_name` is matched case-insensitively against
    /// `self.classes`. This avoids a cross-module dependency on the
    /// `Class` enum in `character/`.
    pub fn is_class_spell(&self, class_name: &str) -> bool {
        let lower = class_name.to_lowercase();
        self.classes.iter().any(|c| *c == lower.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpellSchool {
    Abjuration,
    Conjuration,
    Divination,
    Enchantment,
    Evocation,
    Illusion,
    Necromancy,
    Transmutation,
}

/// How a spell resolves mechanically.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CastingMode {
    /// Ranged spell attack vs AC.
    SpellAttack,
    /// Auto-hit, no roll needed.
    AutoHit,
    /// Targets make a saving throw.
    SaveHalf { save_ability: Ability },
    /// Area effect by HP pool.
    HpPool,
    /// Self-buff.
    SelfBuff,
    /// Ally-target healing/buff (positive effect).
    Heal,
    /// Flavor only, no mechanical effect (utility, out-of-scope mechanics).
    Flavor,
}

// ---- Class-list string constants (internal) ----
//
// Using &'static [&'static str] avoids a cross-module import of the
// `Class` enum (module-isolation rule: spells/ depends only on types/ and
// state/). Class names here are lowercase to match
// `Class::to_string().to_lowercase()`.

const BARD: &str = "bard";
const CLERIC: &str = "cleric";
const DRUID: &str = "druid";
const PALADIN: &str = "paladin";
const RANGER: &str = "ranger";
const SORCERER: &str = "sorcerer";
const WARLOCK: &str = "warlock";
const WIZARD: &str = "wizard";

/// Full spell catalog (cantrip through level 3, SRD 5.1 subset). Each entry
/// carries its SRD tags (ritual, concentration) and class list so the
/// orchestrator can enforce per-class known-spell populations and the
/// ritual/concentration flows. Levels 4+ are intentionally out of scope for
/// the current feature (tracked by a future issue).
pub const SPELLS: &[SpellDef] = &[
    // ===== Cantrips (level 0) =====
    SpellDef { name: "Fire Bolt", level: 0, school: SpellSchool::Evocation,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Prestidigitation", level: 0, school: SpellSchool::Transmutation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Light", level: 0, school: SpellSchool::Evocation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, SORCERER, WIZARD] },
    SpellDef { name: "Mage Hand", level: 0, school: SpellSchool::Conjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Minor Illusion", level: 0, school: SpellSchool::Illusion,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Sacred Flame", level: 0, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: false, ritual: false, classes: &[CLERIC] },
    SpellDef { name: "Guidance", level: 0, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[CLERIC, DRUID] },
    SpellDef { name: "Druidcraft", level: 0, school: SpellSchool::Transmutation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[DRUID] },
    SpellDef { name: "Eldritch Blast", level: 0, school: SpellSchool::Evocation,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[WARLOCK] },
    SpellDef { name: "Vicious Mockery", level: 0, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: false, ritual: false, classes: &[BARD] },

    // ===== Level 1 spells =====
    SpellDef { name: "Magic Missile", level: 1, school: SpellSchool::Evocation,
        casting: CastingMode::AutoHit, concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Burning Hands", level: 1, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: false, ritual: false, classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Sleep", level: 1, school: SpellSchool::Enchantment,
        casting: CastingMode::HpPool, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Shield", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Charm Person", level: 1, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: false, ritual: false,
        classes: &[BARD, DRUID, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Cure Wounds", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, DRUID, PALADIN, RANGER] },
    SpellDef { name: "Detect Magic", level: 1, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: true, ritual: true,
        classes: &[BARD, CLERIC, DRUID, PALADIN, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Disguise Self", level: 1, school: SpellSchool::Illusion,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Expeditious Retreat", level: 1, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Faerie Fire", level: 1, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: true, ritual: false, classes: &[BARD, DRUID] },
    SpellDef { name: "Feather Fall", level: 1, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Find Familiar", level: 1, school: SpellSchool::Conjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[WIZARD] },
    SpellDef { name: "Fog Cloud", level: 1, school: SpellSchool::Conjuration,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[DRUID, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Goodberry", level: 1, school: SpellSchool::Transmutation,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[DRUID, RANGER] },
    SpellDef { name: "Grease", level: 1, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: false, ritual: false, classes: &[WIZARD] },
    SpellDef { name: "Healing Word", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, DRUID] },
    SpellDef { name: "Heroism", level: 1, school: SpellSchool::Enchantment,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[BARD, PALADIN] },
    SpellDef { name: "Hideous Laughter", level: 1, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false, classes: &[BARD, WIZARD] },
    SpellDef { name: "Hunter's Mark", level: 1, school: SpellSchool::Divination,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[RANGER] },
    SpellDef { name: "Identify", level: 1, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[BARD, WIZARD] },
    SpellDef { name: "Inflict Wounds", level: 1, school: SpellSchool::Necromancy,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[CLERIC] },
    SpellDef { name: "Jump", level: 1, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[DRUID, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Longstrider", level: 1, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[BARD, DRUID, RANGER, WIZARD] },
    SpellDef { name: "Mage Armor", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Protection from Evil and Good", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[CLERIC, PALADIN, WARLOCK, WIZARD] },
    SpellDef { name: "Sanctuary", level: 1, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[CLERIC] },
    SpellDef { name: "Speak with Animals", level: 1, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[BARD, DRUID, RANGER] },
    SpellDef { name: "Thunderwave", level: 1, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Constitution },
        concentration: false, ritual: false, classes: &[BARD, DRUID, SORCERER, WIZARD] },
    SpellDef { name: "Unseen Servant", level: 1, school: SpellSchool::Conjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[BARD, WARLOCK, WIZARD] },
    SpellDef { name: "Bless", level: 1, school: SpellSchool::Enchantment,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[CLERIC, PALADIN] },
    SpellDef { name: "Guiding Bolt", level: 1, school: SpellSchool::Evocation,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[CLERIC] },

    // ===== Level 2 spells =====
    SpellDef { name: "Aid", level: 2, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[CLERIC, PALADIN] },
    SpellDef { name: "Alter Self", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Augury", level: 2, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[CLERIC] },
    SpellDef { name: "Barkskin", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[DRUID, RANGER] },
    SpellDef { name: "Blindness/Deafness", level: 2, school: SpellSchool::Necromancy,
        casting: CastingMode::SaveHalf { save_ability: Ability::Constitution },
        concentration: false, ritual: false,
        classes: &[BARD, CLERIC, SORCERER, WIZARD] },
    SpellDef { name: "Blur", level: 2, school: SpellSchool::Illusion,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Calm Emotions", level: 2, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Charisma },
        concentration: true, ritual: false, classes: &[BARD, CLERIC] },
    SpellDef { name: "Crown of Madness", level: 2, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Darkness", level: 2, school: SpellSchool::Evocation,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Darkvision", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[DRUID, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Detect Thoughts", level: 2, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Enhance Ability", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[BARD, CLERIC, DRUID, SORCERER] },
    SpellDef { name: "Enlarge/Reduce", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Constitution },
        concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Find Traps", level: 2, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[CLERIC, DRUID, RANGER] },
    SpellDef { name: "Flaming Sphere", level: 2, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: true, ritual: false,
        classes: &[DRUID, WIZARD] },
    SpellDef { name: "Gentle Repose", level: 2, school: SpellSchool::Necromancy,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[CLERIC, WIZARD] },
    SpellDef { name: "Hold Person", level: 2, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, CLERIC, DRUID, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Invisibility", level: 2, school: SpellSchool::Illusion,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Knock", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Lesser Restoration", level: 2, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, DRUID, PALADIN, RANGER] },
    SpellDef { name: "Levitate", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Constitution },
        concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Magic Mouth", level: 2, school: SpellSchool::Illusion,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[BARD, WIZARD] },
    SpellDef { name: "Misty Step", level: 2, school: SpellSchool::Conjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Pass without Trace", level: 2, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[DRUID, RANGER] },
    SpellDef { name: "Prayer of Healing", level: 2, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[CLERIC] },
    SpellDef { name: "Protection from Poison", level: 2, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[CLERIC, DRUID, PALADIN, RANGER] },
    SpellDef { name: "Scorching Ray", level: 2, school: SpellSchool::Evocation,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "See Invisibility", level: 2, school: SpellSchool::Divination,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Silence", level: 2, school: SpellSchool::Illusion,
        casting: CastingMode::Flavor, concentration: true, ritual: true,
        classes: &[BARD, CLERIC, RANGER] },
    SpellDef { name: "Spider Climb", level: 2, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Spiritual Weapon", level: 2, school: SpellSchool::Evocation,
        casting: CastingMode::SpellAttack, concentration: false, ritual: false,
        classes: &[CLERIC] },
    SpellDef { name: "Suggestion", level: 2, school: SpellSchool::Enchantment,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Web", level: 2, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },

    // ===== Level 3 spells =====
    SpellDef { name: "Animate Dead", level: 3, school: SpellSchool::Necromancy,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[CLERIC, WIZARD] },
    SpellDef { name: "Bestow Curse", level: 3, school: SpellSchool::Necromancy,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, CLERIC, WIZARD] },
    SpellDef { name: "Call Lightning", level: 3, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: true, ritual: false,
        classes: &[DRUID] },
    SpellDef { name: "Clairvoyance", level: 3, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[BARD, CLERIC, SORCERER, WIZARD] },
    SpellDef { name: "Counterspell", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Dispel Magic", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, DRUID, PALADIN, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Fear", level: 3, school: SpellSchool::Illusion,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Fireball", level: 3, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Fly", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Haste", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Hypnotic Pattern", level: 3, school: SpellSchool::Illusion,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Lightning Bolt", level: 3, school: SpellSchool::Evocation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Dexterity },
        concentration: false, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Mass Healing Word", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[CLERIC] },
    SpellDef { name: "Nondetection", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: false, ritual: false,
        classes: &[BARD, RANGER, WIZARD] },
    SpellDef { name: "Plant Growth", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, DRUID, RANGER] },
    SpellDef { name: "Protection from Energy", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::SelfBuff, concentration: true, ritual: false,
        classes: &[CLERIC, DRUID, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Remove Curse", level: 3, school: SpellSchool::Abjuration,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[CLERIC, PALADIN, WARLOCK, WIZARD] },
    SpellDef { name: "Revivify", level: 3, school: SpellSchool::Necromancy,
        casting: CastingMode::Heal, concentration: false, ritual: false,
        classes: &[CLERIC, PALADIN] },
    SpellDef { name: "Sending", level: 3, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, WIZARD] },
    SpellDef { name: "Sleet Storm", level: 3, school: SpellSchool::Conjuration,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[DRUID, SORCERER, WIZARD] },
    SpellDef { name: "Slow", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false,
        classes: &[SORCERER, WIZARD] },
    SpellDef { name: "Speak with Dead", level: 3, school: SpellSchool::Necromancy,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, CLERIC] },
    SpellDef { name: "Speak with Plants", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, DRUID, RANGER] },
    SpellDef { name: "Spirit Guardians", level: 3, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Wisdom },
        concentration: true, ritual: false, classes: &[CLERIC] },
    SpellDef { name: "Stinking Cloud", level: 3, school: SpellSchool::Conjuration,
        casting: CastingMode::SaveHalf { save_ability: Ability::Constitution },
        concentration: true, ritual: false,
        classes: &[BARD, SORCERER, WIZARD] },
    SpellDef { name: "Tiny Hut", level: 3, school: SpellSchool::Evocation,
        casting: CastingMode::Flavor, concentration: false, ritual: true,
        classes: &[BARD, WIZARD] },
    SpellDef { name: "Tongues", level: 3, school: SpellSchool::Divination,
        casting: CastingMode::Flavor, concentration: false, ritual: false,
        classes: &[BARD, CLERIC, SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Vampiric Touch", level: 3, school: SpellSchool::Necromancy,
        casting: CastingMode::SpellAttack, concentration: true, ritual: false,
        classes: &[SORCERER, WARLOCK, WIZARD] },
    SpellDef { name: "Water Breathing", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: true,
        classes: &[DRUID, RANGER, SORCERER, WIZARD] },
    SpellDef { name: "Water Walk", level: 3, school: SpellSchool::Transmutation,
        casting: CastingMode::SelfBuff, concentration: false, ritual: true,
        classes: &[CLERIC, DRUID, RANGER, SORCERER] },
    SpellDef { name: "Wind Wall", level: 3, school: SpellSchool::Evocation,
        casting: CastingMode::Flavor, concentration: true, ritual: false,
        classes: &[DRUID, RANGER] },
];

/// Look up a spell definition by name (case-insensitive).
pub fn find_spell(name: &str) -> Option<&'static SpellDef> {
    let lower = name.to_lowercase();
    SPELLS.iter().find(|s| s.name.to_lowercase() == lower)
}

/// All spells on a given class's list (by lowercase class-name match).
/// The returned slice is filtered from [`SPELLS`]; the caller decides how
/// many to hand out (known-caster limits, prepared-caster lists, etc.).
pub fn spells_for_class(class_name: &str) -> Vec<&'static SpellDef> {
    SPELLS.iter().filter(|s| s.is_class_spell(class_name)).collect()
}

/// Which ability score does this class cast with per SRD 5.1?
///
/// - INT: Wizard
/// - WIS: Cleric, Druid, Ranger
/// - CHA: Bard, Paladin, Sorcerer, Warlock
///
/// Non-casting classes return [`Ability::Intelligence`] as a neutral default
/// (they shouldn't reach a code path that uses this, but a sensible default
/// avoids panics). Matching is case-insensitive.
pub fn spellcasting_ability(class_name: &str) -> Ability {
    match class_name.to_lowercase().as_str() {
        "wizard" => Ability::Intelligence,
        "cleric" | "druid" | "ranger" => Ability::Wisdom,
        "bard" | "paladin" | "sorcerer" | "warlock" => Ability::Charisma,
        _ => Ability::Intelligence,
    }
}

/// Full-caster spell-slot progression per SRD 5.1. Index is `class_level - 1`;
/// each row lists the number of slots per spell level (indices 0..=8 = 1st..9th).
/// Shared by Bard, Cleric, Druid, Sorcerer, and Wizard.
pub const FULL_CASTER_SLOT_TABLE: [[u32; 9]; 20] = [
    // L1
    [2, 0, 0, 0, 0, 0, 0, 0, 0],
    // L2
    [3, 0, 0, 0, 0, 0, 0, 0, 0],
    // L3
    [4, 2, 0, 0, 0, 0, 0, 0, 0],
    // L4
    [4, 3, 0, 0, 0, 0, 0, 0, 0],
    // L5
    [4, 3, 2, 0, 0, 0, 0, 0, 0],
    // L6
    [4, 3, 3, 0, 0, 0, 0, 0, 0],
    // L7
    [4, 3, 3, 1, 0, 0, 0, 0, 0],
    // L8
    [4, 3, 3, 2, 0, 0, 0, 0, 0],
    // L9
    [4, 3, 3, 3, 1, 0, 0, 0, 0],
    // L10
    [4, 3, 3, 3, 2, 0, 0, 0, 0],
    // L11
    [4, 3, 3, 3, 2, 1, 0, 0, 0],
    // L12
    [4, 3, 3, 3, 2, 1, 0, 0, 0],
    // L13
    [4, 3, 3, 3, 2, 1, 1, 0, 0],
    // L14
    [4, 3, 3, 3, 2, 1, 1, 0, 0],
    // L15
    [4, 3, 3, 3, 2, 1, 1, 1, 0],
    // L16
    [4, 3, 3, 3, 2, 1, 1, 1, 0],
    // L17
    [4, 3, 3, 3, 2, 1, 1, 1, 1],
    // L18
    [4, 3, 3, 3, 3, 1, 1, 1, 1],
    // L19
    [4, 3, 3, 3, 3, 2, 1, 1, 1],
    // L20
    [4, 3, 3, 3, 3, 2, 2, 1, 1],
];

/// Half-caster spell-slot progression per SRD 5.1. Used by Paladin and
/// Ranger. Slots unlock at class level 2. Index is `class_level - 1`.
pub const HALF_CASTER_SLOT_TABLE: [[u32; 9]; 20] = [
    // L1 (no slots)
    [0, 0, 0, 0, 0, 0, 0, 0, 0],
    // L2
    [2, 0, 0, 0, 0, 0, 0, 0, 0],
    // L3
    [3, 0, 0, 0, 0, 0, 0, 0, 0],
    // L4
    [3, 0, 0, 0, 0, 0, 0, 0, 0],
    // L5
    [4, 2, 0, 0, 0, 0, 0, 0, 0],
    // L6
    [4, 2, 0, 0, 0, 0, 0, 0, 0],
    // L7
    [4, 3, 0, 0, 0, 0, 0, 0, 0],
    // L8
    [4, 3, 0, 0, 0, 0, 0, 0, 0],
    // L9
    [4, 3, 2, 0, 0, 0, 0, 0, 0],
    // L10
    [4, 3, 2, 0, 0, 0, 0, 0, 0],
    // L11
    [4, 3, 3, 0, 0, 0, 0, 0, 0],
    // L12
    [4, 3, 3, 0, 0, 0, 0, 0, 0],
    // L13
    [4, 3, 3, 1, 0, 0, 0, 0, 0],
    // L14
    [4, 3, 3, 1, 0, 0, 0, 0, 0],
    // L15
    [4, 3, 3, 2, 0, 0, 0, 0, 0],
    // L16
    [4, 3, 3, 2, 0, 0, 0, 0, 0],
    // L17
    [4, 3, 3, 3, 1, 0, 0, 0, 0],
    // L18
    [4, 3, 3, 3, 1, 0, 0, 0, 0],
    // L19
    [4, 3, 3, 3, 2, 0, 0, 0, 0],
    // L20
    [4, 3, 3, 3, 2, 0, 0, 0, 0],
];

/// Warlock Pact Magic slots per SRD 5.1. Warlocks have a small number of
/// high-level slots that refresh on a short rest. Index `class_level - 1`.
/// Slot level equals the highest entry index in each row.
pub const WARLOCK_SLOT_TABLE: [[u32; 9]; 20] = [
    // L1:  1 x L1
    [1, 0, 0, 0, 0, 0, 0, 0, 0],
    // L2:  2 x L1
    [2, 0, 0, 0, 0, 0, 0, 0, 0],
    // L3:  2 x L2
    [0, 2, 0, 0, 0, 0, 0, 0, 0],
    // L4:  2 x L2
    [0, 2, 0, 0, 0, 0, 0, 0, 0],
    // L5:  2 x L3
    [0, 0, 2, 0, 0, 0, 0, 0, 0],
    // L6:  2 x L3
    [0, 0, 2, 0, 0, 0, 0, 0, 0],
    // L7:  2 x L4
    [0, 0, 0, 2, 0, 0, 0, 0, 0],
    // L8:  2 x L4
    [0, 0, 0, 2, 0, 0, 0, 0, 0],
    // L9:  2 x L5
    [0, 0, 0, 0, 2, 0, 0, 0, 0],
    // L10: 2 x L5
    [0, 0, 0, 0, 2, 0, 0, 0, 0],
    // L11: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L12: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L13: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L14: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L15: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L16: 3 x L5
    [0, 0, 0, 0, 3, 0, 0, 0, 0],
    // L17: 4 x L5
    [0, 0, 0, 0, 4, 0, 0, 0, 0],
    // L18: 4 x L5
    [0, 0, 0, 0, 4, 0, 0, 0, 0],
    // L19: 4 x L5
    [0, 0, 0, 0, 4, 0, 0, 0, 0],
    // L20: 4 x L5
    [0, 0, 0, 0, 4, 0, 0, 0, 0],
];

/// Compute the slot vector for a caster at a given level. Returns a
/// trimmed `Vec<i32>` (no trailing zeros) so existing state serialization
/// stays compact. Lookups clamp the level to `1..=20`.
///
/// The matching rules:
/// - Bard/Cleric/Druid/Sorcerer/Wizard -> full-caster table
/// - Paladin/Ranger -> half-caster table
/// - Warlock -> Pact Magic table
/// - Others -> empty vector
pub fn slots_for(class_name: &str, level: u32) -> Vec<i32> {
    let idx = level.clamp(1, 20) as usize - 1;
    let row: &[u32; 9] = match class_name.to_lowercase().as_str() {
        "bard" | "cleric" | "druid" | "sorcerer" | "wizard" => &FULL_CASTER_SLOT_TABLE[idx],
        "paladin" | "ranger" => &HALF_CASTER_SLOT_TABLE[idx],
        "warlock" => &WARLOCK_SLOT_TABLE[idx],
        _ => return Vec::new(),
    };
    let mut out: Vec<i32> = row.iter().map(|c| *c as i32).collect();
    // Trim trailing zeros so vec reflects the highest level actually accessible.
    while out.last().copied() == Some(0) {
        out.pop();
    }
    out
}

/// Compute spell attack modifier: ability mod + proficiency bonus.
pub fn spell_attack_modifier(ability_score: i32, proficiency_bonus: i32) -> i32 {
    Ability::modifier(ability_score) + proficiency_bonus
}

/// Compute spell save DC: 8 + ability mod + proficiency bonus.
pub fn spell_save_dc(ability_score: i32, proficiency_bonus: i32) -> i32 {
    8 + Ability::modifier(ability_score) + proficiency_bonus
}

/// Result of a spell attack roll.
#[derive(Debug, Clone)]
pub struct SpellAttackResult {
    pub roll: i32,
    pub modifier: i32,
    pub total: i32,
    pub hit: bool,
    pub natural_20: bool,
    pub natural_1: bool,
}

/// Roll a spell attack against a target AC.
pub fn roll_spell_attack(
    rng: &mut impl Rng,
    ability_score: i32,
    proficiency_bonus: i32,
    target_ac: i32,
) -> SpellAttackResult {
    let roll = roll_d20(rng);
    let modifier = spell_attack_modifier(ability_score, proficiency_bonus);
    let total = roll + modifier;
    let natural_20 = roll == 20;
    let natural_1 = roll == 1;
    let hit = natural_20 || (!natural_1 && total >= target_ac);
    SpellAttackResult { roll, modifier, total, hit, natural_20, natural_1 }
}

/// Result of a spell save.
#[derive(Debug, Clone)]
pub struct SpellSaveResult {
    pub roll: i32,
    pub modifier: i32,
    pub total: i32,
    pub dc: i32,
    pub saved: bool,
}

/// Roll a saving throw against the caster's spell save DC.
pub fn roll_spell_save(
    rng: &mut impl Rng,
    save_ability_score: i32,
    save_proficiency_bonus: i32,
    is_proficient: bool,
    dc: i32,
) -> SpellSaveResult {
    let roll = roll_d20(rng);
    let modifier = Ability::modifier(save_ability_score) + if is_proficient { save_proficiency_bonus } else { 0 };
    let total = roll + modifier;
    SpellSaveResult { roll, modifier, total, dc, saved: total >= dc }
}

/// Outcome of a complete spell cast.
#[derive(Debug, Clone)]
pub enum CastOutcome {
    /// Not a spellcaster.
    NotACaster,
    /// Spell not known.
    UnknownSpell,
    /// No spell slots remaining.
    NoSlots,
    /// Spell not usable outside combat.
    NotInCombat,
    /// Fire Bolt: spell attack result + damage.
    FireBolt {
        attack: SpellAttackResult,
        damage: i32,
    },
    /// Prestidigitation flavor text.
    Prestidigitation,
    /// Magic Missile: auto-hit damage.
    MagicMissile {
        darts: Vec<i32>,
        total_damage: i32,
    },
    /// Burning Hands: per-target save results + damage.
    BurningHands {
        total_rolled: i32,
        half_damage: i32,
        dc: i32,
        results: Vec<BurningHandsTarget>,
    },
    /// Sleep: HP pool and affected targets.
    SleepResult {
        hp_pool: i32,
        affected: Vec<SleepTarget>,
    },
    /// Shield: AC bonus applied.
    ShieldCast {
        ac_bonus: i32,
    },
}

#[derive(Debug, Clone)]
pub struct BurningHandsTarget {
    pub name: String,
    pub save_result: SpellSaveResult,
    pub damage_taken: i32,
}

#[derive(Debug, Clone)]
pub struct SleepTarget {
    pub name: String,
    pub hp: i32,
}

/// Information about an enemy target needed for spell resolution.
/// This struct avoids importing combat or NPC types directly.
#[derive(Debug, Clone)]
pub struct SpellTarget {
    pub id: u32,
    pub name: String,
    pub ac: i32,
    pub current_hp: i32,
    pub ability_scores: std::collections::HashMap<Ability, i32>,
    pub proficiency_bonus: i32,
    pub save_proficiencies: Vec<Ability>,
    pub distance: u32,
}

/// Resolve a Fire Bolt cast against a single target.
pub fn resolve_fire_bolt(
    rng: &mut impl Rng,
    ability_score: i32,
    proficiency_bonus: i32,
    target_ac: i32,
) -> CastOutcome {
    let attack = roll_spell_attack(rng, ability_score, proficiency_bonus, target_ac);
    let damage = if attack.hit {
        let rolls = roll_dice(rng, 1, 10);
        let base: i32 = rolls.iter().sum();
        if attack.natural_20 { base * 2 } else { base }
    } else {
        0
    };
    CastOutcome::FireBolt { attack, damage }
}

/// Resolve Magic Missile (auto-hit, 3 darts of 1d4+1).
pub fn resolve_magic_missile(rng: &mut impl Rng) -> CastOutcome {
    let mut darts = Vec::new();
    for _ in 0..3 {
        let rolls = roll_dice(rng, 1, 4);
        darts.push(rolls.iter().sum::<i32>() + 1);
    }
    let total_damage = darts.iter().sum();
    CastOutcome::MagicMissile { darts, total_damage }
}

/// Resolve Burning Hands against all targets within 5 ft.
pub fn resolve_burning_hands(
    rng: &mut impl Rng,
    caster_ability_score: i32,
    caster_proficiency_bonus: i32,
    targets: &[SpellTarget],
) -> CastOutcome {
    let dc = spell_save_dc(caster_ability_score, caster_proficiency_bonus);
    let damage_rolls = roll_dice(rng, 3, 6);
    let total_rolled: i32 = damage_rolls.iter().sum();
    let half_damage = total_rolled / 2;

    let melee_targets: Vec<&SpellTarget> = targets.iter().filter(|t| t.distance <= 5).collect();

    let mut results = Vec::new();
    for target in melee_targets {
        let dex_score = target.ability_scores.get(&Ability::Dexterity).copied().unwrap_or(10);
        let is_prof = target.save_proficiencies.contains(&Ability::Dexterity);
        let save = roll_spell_save(rng, dex_score, target.proficiency_bonus, is_prof, dc);
        let damage_taken = if save.saved { half_damage } else { total_rolled };
        results.push(BurningHandsTarget {
            name: target.name.clone(),
            save_result: save,
            damage_taken,
        });
    }

    CastOutcome::BurningHands { total_rolled, half_damage, dc, results }
}

/// Resolve Sleep spell (5d8 HP pool, weakest first).
pub fn resolve_sleep(
    rng: &mut impl Rng,
    targets: &[SpellTarget],
) -> CastOutcome {
    let pool_rolls = roll_dice(rng, 5, 8);
    let mut hp_pool: i32 = pool_rolls.iter().sum();

    // Sort targets by current HP (weakest first)
    let mut sorted: Vec<&SpellTarget> = targets.iter().collect();
    sorted.sort_by_key(|t| t.current_hp);

    let mut affected = Vec::new();
    for target in sorted {
        if target.current_hp > 0 && target.current_hp <= hp_pool {
            hp_pool -= target.current_hp;
            affected.push(SleepTarget {
                name: target.name.clone(),
                hp: target.current_hp,
            });
        }
    }

    let total_pool: i32 = pool_rolls.iter().sum();
    CastOutcome::SleepResult { hp_pool: total_pool, affected }
}

/// Resolve Shield spell (+5 AC self-buff).
pub fn resolve_shield() -> CastOutcome {
    CastOutcome::ShieldCast { ac_bonus: 5 }
}

/// Format the player's known spells and remaining spell slots for display.
/// Returns lines suitable for the `spells` command output.
pub fn format_known_spells(
    known_spells: &[String],
    spell_slots_remaining: &[i32],
    spell_slots_max: &[i32],
) -> Vec<String> {
    if known_spells.is_empty() {
        return vec!["You don't know any spells.".to_string()];
    }

    let mut lines = Vec::new();
    lines.push("=== Known Spells ===".to_string());

    // Group by spell level (0 = cantrips, 1..=9 = leveled)
    for level in 0..=9u32 {
        let in_level: Vec<&String> = known_spells
            .iter()
            .filter(|name| find_spell(name).map_or(false, |def| def.level == level))
            .collect();
        if in_level.is_empty() { continue; }

        lines.push(String::new());
        let header = if level == 0 {
            "Cantrips (at will):".to_string()
        } else {
            format!("Level {} Spells:", level)
        };
        lines.push(header);

        for name in &in_level {
            let def = find_spell(name);
            let mut tags: Vec<&'static str> = Vec::new();
            if let Some(d) = def {
                if d.concentration { tags.push("C"); }
                if d.ritual { tags.push("R"); }
            }
            let suffix = if tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", tags.join(","))
            };
            lines.push(format!("  - {}{}", name, suffix));
        }
    }

    // Spell slots
    if !spell_slots_max.is_empty() {
        lines.push(String::new());
        lines.push("Spell Slots:".to_string());
        for (i, (remaining, max)) in spell_slots_remaining.iter().zip(spell_slots_max.iter()).enumerate() {
            if *max > 0 {
                lines.push(format!("  Level {}: {}/{}", i + 1, remaining, max));
            }
        }
    }

    lines
}

/// Check if a character can cast and consume a slot. Returns true if the slot was consumed.
/// For cantrips (level 0), always returns true without consuming slots.
pub fn consume_spell_slot(
    spell_level: u32,
    slots_remaining: &mut Vec<i32>,
) -> bool {
    if spell_level == 0 {
        return true; // cantrips don't consume slots
    }
    let idx = (spell_level - 1) as usize;
    if idx >= slots_remaining.len() || slots_remaining[idx] <= 0 {
        return false;
    }
    slots_remaining[idx] -= 1;
    true
}

/// Compute the concentration-save DC on taking damage while concentrating.
///
/// Per SRD 5.1: DC = max(10, damage_taken / 2). The caster makes a
/// Constitution save against this DC; failure drops the concentration.
pub fn concentration_save_dc(damage_taken: i32) -> i32 {
    (damage_taken / 2).max(10)
}

/// Outcome of starting a new concentration spell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConcentrationStart {
    /// No prior concentration; the new spell is now the active concentration.
    Started,
    /// A prior concentration spell was dropped. Carries the dropped
    /// spell's name so narration can mention it.
    ReplacedPrior(String),
}

/// Apply starting a new concentration spell to the caster's state.
///
/// Mutates `current` to `Some(new_spell)` and returns whether there was a
/// prior concentration that got dropped. The caller is responsible for
/// narrating.
pub fn begin_concentration(
    current: &mut Option<String>,
    new_spell: &str,
) -> ConcentrationStart {
    let prior = current.take();
    *current = Some(new_spell.to_string());
    match prior {
        Some(name) if name.to_lowercase() != new_spell.to_lowercase() => {
            ConcentrationStart::ReplacedPrior(name)
        }
        _ => ConcentrationStart::Started,
    }
}

/// Resolve a concentration-maintenance save result. `saved == false` means
/// the caster drops concentration.
pub fn resolve_concentration_save(
    rng: &mut impl Rng,
    con_score: i32,
    con_save_prof: bool,
    proficiency_bonus: i32,
    damage_taken: i32,
) -> SpellSaveResult {
    let dc = concentration_save_dc(damage_taken);
    roll_spell_save(rng, con_score, proficiency_bonus, con_save_prof, dc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;
    use std::collections::HashMap;

    fn test_target(name: &str, ac: i32, hp: i32, dex: i32, distance: u32) -> SpellTarget {
        let mut scores = HashMap::new();
        scores.insert(Ability::Dexterity, dex);
        SpellTarget {
            id: 0,
            name: name.to_string(),
            ac,
            current_hp: hp,
            ability_scores: scores,
            proficiency_bonus: 2,
            save_proficiencies: Vec::new(),
            distance,
        }
    }

    #[test]
    fn test_find_spell_case_insensitive() {
        assert!(find_spell("fire bolt").is_some());
        assert!(find_spell("Fire Bolt").is_some());
        assert!(find_spell("MAGIC MISSILE").is_some());
        assert!(find_spell("nonexistent").is_none());
    }

    #[test]
    fn test_spell_attack_modifier() {
        // Ability 16 (+3) + prof 2 = +5
        assert_eq!(spell_attack_modifier(16, 2), 5);
        // Ability 10 (+0) + prof 2 = +2
        assert_eq!(spell_attack_modifier(10, 2), 2);
    }

    #[test]
    fn test_spell_save_dc() {
        // 8 + ability 16 (+3) + prof 2 = 13
        assert_eq!(spell_save_dc(16, 2), 13);
        // 8 + ability 10 (+0) + prof 2 = 10
        assert_eq!(spell_save_dc(10, 2), 10);
    }

    #[test]
    fn test_fire_bolt_rolls_attack_and_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = resolve_fire_bolt(&mut rng, 16, 2, 12);
        match result {
            CastOutcome::FireBolt { attack, damage } => {
                assert!(attack.roll >= 1 && attack.roll <= 20);
                assert_eq!(attack.modifier, 5);
                if attack.hit {
                    assert!(damage >= 1 && damage <= 20);
                } else {
                    assert_eq!(damage, 0);
                }
            }
            _ => panic!("Expected FireBolt outcome"),
        }
    }

    #[test]
    fn test_magic_missile_auto_hit() {
        let mut rng = StdRng::seed_from_u64(42);
        let result = resolve_magic_missile(&mut rng);
        match result {
            CastOutcome::MagicMissile { darts, total_damage } => {
                assert_eq!(darts.len(), 3);
                for dart in &darts {
                    assert!(*dart >= 2 && *dart <= 5, "Dart {} out of 1d4+1 range", dart);
                }
                assert_eq!(total_damage, darts.iter().sum::<i32>());
            }
            _ => panic!("Expected MagicMissile outcome"),
        }
    }

    #[test]
    fn test_burning_hands_only_hits_melee_targets() {
        let mut rng = StdRng::seed_from_u64(42);
        let targets = vec![
            test_target("Goblin", 12, 7, 10, 5),
            test_target("Archer", 13, 10, 14, 30),
        ];
        let result = resolve_burning_hands(&mut rng, 16, 2, &targets);
        match result {
            CastOutcome::BurningHands { results, dc, .. } => {
                assert_eq!(dc, 13);
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].name, "Goblin");
            }
            _ => panic!("Expected BurningHands outcome"),
        }
    }

    #[test]
    fn test_burning_hands_save_half_damage() {
        let mut rng = StdRng::seed_from_u64(42);
        let targets = vec![test_target("Goblin", 12, 7, 10, 5)];
        let result = resolve_burning_hands(&mut rng, 16, 2, &targets);
        match result {
            CastOutcome::BurningHands { total_rolled, half_damage, results, .. } => {
                assert!(total_rolled >= 3 && total_rolled <= 18);
                assert_eq!(half_damage, total_rolled / 2);
                let target = &results[0];
                if target.save_result.saved {
                    assert_eq!(target.damage_taken, half_damage);
                } else {
                    assert_eq!(target.damage_taken, total_rolled);
                }
            }
            _ => panic!("Expected BurningHands outcome"),
        }
    }

    #[test]
    fn test_sleep_targets_weakest_first() {
        let mut rng = StdRng::seed_from_u64(100);
        let targets = vec![
            test_target("Rat", 10, 3, 10, 5),
            test_target("Goblin", 12, 7, 10, 5),
            test_target("Ogre", 11, 59, 8, 5),
        ];
        let result = resolve_sleep(&mut rng, &targets);
        match result {
            CastOutcome::SleepResult { hp_pool, affected } => {
                assert!(hp_pool >= 5 && hp_pool <= 40);
                if !affected.is_empty() {
                    assert_eq!(affected[0].name, "Rat");
                }
                assert!(!affected.iter().any(|t| t.name == "Ogre"));
            }
            _ => panic!("Expected SleepResult outcome"),
        }
    }

    #[test]
    fn test_shield_gives_5_ac() {
        let result = resolve_shield();
        match result {
            CastOutcome::ShieldCast { ac_bonus } => assert_eq!(ac_bonus, 5),
            _ => panic!("Expected ShieldCast outcome"),
        }
    }

    #[test]
    fn test_consume_spell_slot_cantrip() {
        let mut slots = vec![2];
        assert!(consume_spell_slot(0, &mut slots));
        assert_eq!(slots, vec![2]);
    }

    #[test]
    fn test_consume_spell_slot_level1() {
        let mut slots = vec![2];
        assert!(consume_spell_slot(1, &mut slots));
        assert_eq!(slots, vec![1]);
        assert!(consume_spell_slot(1, &mut slots));
        assert_eq!(slots, vec![0]);
        assert!(!consume_spell_slot(1, &mut slots));
    }

    #[test]
    fn test_consume_spell_slot_no_slots_at_level() {
        let mut slots: Vec<i32> = Vec::new();
        assert!(!consume_spell_slot(1, &mut slots));
    }

    #[test]
    fn test_fire_bolt_is_cantrip() {
        let spell = find_spell("Fire Bolt").unwrap();
        assert_eq!(spell.level, 0);
        assert_eq!(spell.casting, CastingMode::SpellAttack);
    }

    #[test]
    fn test_prestidigitation_is_flavor() {
        let spell = find_spell("Prestidigitation").unwrap();
        assert_eq!(spell.level, 0);
        assert_eq!(spell.casting, CastingMode::Flavor);
    }

    #[test]
    fn test_format_known_spells_wizard_mvp() {
        let known = vec![
            "Fire Bolt".to_string(),
            "Prestidigitation".to_string(),
            "Magic Missile".to_string(),
            "Burning Hands".to_string(),
            "Sleep".to_string(),
            "Shield".to_string(),
        ];
        let slots_remaining = vec![2];
        let slots_max = vec![2];

        let lines = format_known_spells(&known, &slots_remaining, &slots_max);
        let text = lines.join("\n");

        assert!(text.contains("Known Spells"));
        assert!(text.contains("Cantrips (at will)"));
        assert!(text.contains("Fire Bolt"));
        assert!(text.contains("Prestidigitation"));
        assert!(text.contains("Level 1 Spells"));
        assert!(text.contains("Magic Missile"));
        assert!(text.contains("Spell Slots"));
        assert!(text.contains("Level 1: 2/2"));
    }

    #[test]
    fn test_format_known_spells_empty() {
        let lines = format_known_spells(&[], &[], &[]);
        assert_eq!(lines, vec!["You don't know any spells."]);
    }

    #[test]
    fn test_format_known_spells_after_slot_use() {
        let known = vec![
            "Fire Bolt".to_string(),
            "Magic Missile".to_string(),
        ];
        let slots_remaining = vec![1];
        let slots_max = vec![2];

        let lines = format_known_spells(&known, &slots_remaining, &slots_max);
        let text = lines.join("\n");

        assert!(text.contains("Level 1: 1/2"));
    }

    // ---- SpellDef extension tests (feat/expanded-spell-catalog) ----

    #[test]
    fn test_spell_catalog_covers_levels_0_to_3() {
        // We expect cantrips, 1st, 2nd, and 3rd-level spells in the catalog.
        let levels: std::collections::HashSet<u32> = SPELLS.iter().map(|s| s.level).collect();
        assert!(levels.contains(&0), "catalog must include cantrips");
        assert!(levels.contains(&1), "catalog must include L1 spells");
        assert!(levels.contains(&2), "catalog must include L2 spells");
        assert!(levels.contains(&3), "catalog must include L3 spells");
    }

    #[test]
    fn test_spell_school_has_all_eight_schools() {
        // All eight schools should be representable; sanity-check every variant
        // serializes to a non-error string.
        for school in [
            SpellSchool::Abjuration, SpellSchool::Conjuration, SpellSchool::Divination,
            SpellSchool::Enchantment, SpellSchool::Evocation, SpellSchool::Illusion,
            SpellSchool::Necromancy, SpellSchool::Transmutation,
        ] {
            let json = serde_json::to_string(&school).unwrap();
            assert!(!json.is_empty());
        }
    }

    #[test]
    fn test_spell_def_has_concentration_ritual_classes() {
        // Detect Magic -- ritual AND concentration.
        let detect = find_spell("Detect Magic").expect("Detect Magic must exist in catalog");
        assert!(detect.ritual, "Detect Magic has the Ritual tag");
        assert!(detect.concentration, "Detect Magic requires concentration");
        assert!(!detect.classes.is_empty(), "Detect Magic belongs to at least one class");

        // Fireball -- no ritual, no concentration.
        let fireball = find_spell("Fireball").expect("Fireball must exist in catalog");
        assert!(!fireball.ritual);
        assert!(!fireball.concentration);

        // Hunter's Mark -- concentration, not ritual.
        let hm = find_spell("Hunter's Mark").expect("Hunter's Mark must exist in catalog");
        assert!(!hm.ritual);
        assert!(hm.concentration);
    }

    #[test]
    fn test_class_membership_matches_expected() {
        // Wizard-only ritual
        let find_fam = find_spell("Find Familiar").unwrap();
        assert!(find_fam.is_class_spell("Wizard"));
        assert!(find_fam.is_class_spell("wizard"));
        assert!(!find_fam.is_class_spell("Cleric"));

        // Cleric-only combat spell
        let sac = find_spell("Sacred Flame").unwrap();
        assert!(sac.is_class_spell("Cleric"));
        assert!(!sac.is_class_spell("Wizard"));

        // Ranger signature
        let hm = find_spell("Hunter's Mark").unwrap();
        assert!(hm.is_class_spell("Ranger"));
        assert!(!hm.is_class_spell("Paladin"));
    }

    #[test]
    fn test_spells_for_class_contains_expected_signatures() {
        let wizard = spells_for_class("Wizard");
        let names: Vec<&'static str> = wizard.iter().map(|s| s.name).collect();
        assert!(names.contains(&"Fire Bolt"));
        assert!(names.contains(&"Fireball"));
        assert!(names.contains(&"Mage Armor"));
        // Not Wizard-list
        assert!(!names.contains(&"Sacred Flame"));
        assert!(!names.contains(&"Cure Wounds"));

        let cleric = spells_for_class("Cleric");
        let cnames: Vec<&'static str> = cleric.iter().map(|s| s.name).collect();
        assert!(cnames.contains(&"Sacred Flame"));
        assert!(cnames.contains(&"Cure Wounds"));
        assert!(cnames.contains(&"Spiritual Weapon"));
    }

    #[test]
    fn test_spells_for_class_returns_empty_for_non_caster() {
        assert!(spells_for_class("Fighter").is_empty());
        assert!(spells_for_class("Barbarian").is_empty());
        assert!(spells_for_class("Rogue").is_empty());
        assert!(spells_for_class("Monk").is_empty());
    }

    // ---- Spellcasting ability per class ----

    #[test]
    fn test_spellcasting_ability_wizard_is_int() {
        assert_eq!(spellcasting_ability("Wizard"), Ability::Intelligence);
        assert_eq!(spellcasting_ability("wizard"), Ability::Intelligence);
    }

    #[test]
    fn test_spellcasting_ability_cleric_druid_ranger_is_wis() {
        assert_eq!(spellcasting_ability("Cleric"), Ability::Wisdom);
        assert_eq!(spellcasting_ability("Druid"), Ability::Wisdom);
        assert_eq!(spellcasting_ability("Ranger"), Ability::Wisdom);
    }

    #[test]
    fn test_spellcasting_ability_cha_casters() {
        assert_eq!(spellcasting_ability("Bard"), Ability::Charisma);
        assert_eq!(spellcasting_ability("Paladin"), Ability::Charisma);
        assert_eq!(spellcasting_ability("Sorcerer"), Ability::Charisma);
        assert_eq!(spellcasting_ability("Warlock"), Ability::Charisma);
    }

    // ---- Slot progression tables ----

    #[test]
    fn test_full_caster_slots_level_1() {
        assert_eq!(slots_for("Wizard", 1), vec![2]);
        assert_eq!(slots_for("Bard", 1), vec![2]);
        assert_eq!(slots_for("Cleric", 1), vec![2]);
        assert_eq!(slots_for("Druid", 1), vec![2]);
        assert_eq!(slots_for("Sorcerer", 1), vec![2]);
    }

    #[test]
    fn test_full_caster_slots_level_5() {
        // L5 full caster: 4 / 3 / 2
        assert_eq!(slots_for("Wizard", 5), vec![4, 3, 2]);
    }

    #[test]
    fn test_full_caster_slots_level_20() {
        // L20 full caster: 4/3/3/3/3/2/2/1/1
        let slots = slots_for("Wizard", 20);
        assert_eq!(slots, vec![4, 3, 3, 3, 3, 2, 2, 1, 1]);
    }

    #[test]
    fn test_half_caster_slots() {
        // Paladin L1: no slots
        assert_eq!(slots_for("Paladin", 1), Vec::<i32>::new());
        // Paladin L2: 2 x L1
        assert_eq!(slots_for("Paladin", 2), vec![2]);
        // Ranger L5: 4 x L1, 2 x L2
        assert_eq!(slots_for("Ranger", 5), vec![4, 2]);
    }

    #[test]
    fn test_warlock_pact_magic_slots() {
        // L1: one L1 slot
        assert_eq!(slots_for("Warlock", 1), vec![1]);
        // L3: zero L1, two L2 slots
        assert_eq!(slots_for("Warlock", 3), vec![0, 2]);
        // L5: zero L1/L2, two L3 slots
        assert_eq!(slots_for("Warlock", 5), vec![0, 0, 2]);
    }

    #[test]
    fn test_non_caster_has_no_slots() {
        assert_eq!(slots_for("Fighter", 1), Vec::<i32>::new());
        assert_eq!(slots_for("Fighter", 20), Vec::<i32>::new());
        assert_eq!(slots_for("Barbarian", 5), Vec::<i32>::new());
        assert_eq!(slots_for("Rogue", 10), Vec::<i32>::new());
    }

    #[test]
    fn test_slots_for_clamps_level() {
        // Level 0 and huge levels should behave gracefully (clamp into 1..=20).
        assert_eq!(slots_for("Wizard", 0), slots_for("Wizard", 1));
        assert_eq!(slots_for("Wizard", 25), slots_for("Wizard", 20));
    }

    // ---- Format includes new labels ----

    #[test]
    fn test_format_known_spells_tags_concentration_and_ritual() {
        let known = vec![
            "Hunter's Mark".to_string(),
            "Detect Magic".to_string(),
        ];
        let lines = format_known_spells(&known, &[2, 0], &[2, 0]);
        let text = lines.join("\n");
        // Hunter's Mark is concentration but not ritual -> "[C]"
        assert!(text.contains("Hunter's Mark [C]"), "Expected 'Hunter's Mark [C]' in:\n{}", text);
        // Detect Magic is both -> "[C,R]"
        assert!(text.contains("Detect Magic [C,R]"), "Expected 'Detect Magic [C,R]' in:\n{}", text);
    }

    // ---- Concentration ----

    #[test]
    fn test_concentration_save_dc_floor_is_10() {
        // Tiny damage: DC still 10.
        assert_eq!(concentration_save_dc(1), 10);
        assert_eq!(concentration_save_dc(19), 10);
    }

    #[test]
    fn test_concentration_save_dc_half_for_big_damage() {
        // 30 damage -> DC 15.
        assert_eq!(concentration_save_dc(30), 15);
        // 100 damage -> DC 50.
        assert_eq!(concentration_save_dc(100), 50);
        // 20 damage -> DC 10 (half = 10, floor also 10).
        assert_eq!(concentration_save_dc(20), 10);
    }

    #[test]
    fn test_begin_concentration_from_none_starts() {
        let mut current: Option<String> = None;
        let result = begin_concentration(&mut current, "Bless");
        assert_eq!(result, ConcentrationStart::Started);
        assert_eq!(current, Some("Bless".to_string()));
    }

    #[test]
    fn test_begin_concentration_replaces_prior() {
        let mut current = Some("Bless".to_string());
        let result = begin_concentration(&mut current, "Hold Person");
        match result {
            ConcentrationStart::ReplacedPrior(name) => assert_eq!(name, "Bless"),
            _ => panic!("expected ReplacedPrior"),
        }
        assert_eq!(current, Some("Hold Person".to_string()));
    }

    #[test]
    fn test_begin_concentration_same_spell_not_replaced() {
        // Re-casting the same concentration spell should not report a replacement.
        let mut current = Some("Bless".to_string());
        let result = begin_concentration(&mut current, "bless"); // case-insensitive
        assert_eq!(result, ConcentrationStart::Started);
    }

    #[test]
    fn test_resolve_concentration_save_uses_con() {
        // Sanity: DC is driven by damage, save uses CON score.
        let mut rng = StdRng::seed_from_u64(5);
        let save = resolve_concentration_save(&mut rng, 14, true, 2, 12);
        // damage 12 -> DC 10. CON 14 (+2) + prof 2 = +4.
        assert_eq!(save.dc, 10);
        assert_eq!(save.modifier, 4);
    }
}
