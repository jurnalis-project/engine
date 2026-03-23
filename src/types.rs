use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ability {
    Strength,
    Dexterity,
    Constitution,
    Intelligence,
    Wisdom,
    Charisma,
}

impl Ability {
    pub fn all() -> &'static [Ability] {
        &[
            Ability::Strength,
            Ability::Dexterity,
            Ability::Constitution,
            Ability::Intelligence,
            Ability::Wisdom,
            Ability::Charisma,
        ]
    }

    pub fn modifier(score: i32) -> i32 {
        ((score - 10) as f32 / 2.0).floor() as i32
    }
}

impl std::fmt::Display for Ability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Ability::Strength => write!(f, "STR"),
            Ability::Dexterity => write!(f, "DEX"),
            Ability::Constitution => write!(f, "CON"),
            Ability::Intelligence => write!(f, "INT"),
            Ability::Wisdom => write!(f, "WIS"),
            Ability::Charisma => write!(f, "CHA"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Skill {
    Athletics,
    Acrobatics,
    SleightOfHand,
    Stealth,
    Arcana,
    History,
    Investigation,
    Nature,
    Religion,
    AnimalHandling,
    Insight,
    Medicine,
    Perception,
    Survival,
    Deception,
    Intimidation,
    Performance,
    Persuasion,
}

impl Skill {
    pub fn ability(&self) -> Ability {
        match self {
            Skill::Athletics => Ability::Strength,
            Skill::Acrobatics | Skill::SleightOfHand | Skill::Stealth => Ability::Dexterity,
            Skill::Arcana | Skill::History | Skill::Investigation | Skill::Nature | Skill::Religion => Ability::Intelligence,
            Skill::AnimalHandling | Skill::Insight | Skill::Medicine | Skill::Perception | Skill::Survival => Ability::Wisdom,
            Skill::Deception | Skill::Intimidation | Skill::Performance | Skill::Persuasion => Ability::Charisma,
        }
    }

    pub fn all() -> &'static [Skill] {
        &[
            Skill::Athletics,
            Skill::Acrobatics,
            Skill::SleightOfHand,
            Skill::Stealth,
            Skill::Arcana,
            Skill::History,
            Skill::Investigation,
            Skill::Nature,
            Skill::Religion,
            Skill::AnimalHandling,
            Skill::Insight,
            Skill::Medicine,
            Skill::Perception,
            Skill::Survival,
            Skill::Deception,
            Skill::Intimidation,
            Skill::Performance,
            Skill::Persuasion,
        ]
    }
}

impl std::fmt::Display for Skill {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Skill::Athletics => write!(f, "Athletics"),
            Skill::Acrobatics => write!(f, "Acrobatics"),
            Skill::SleightOfHand => write!(f, "Sleight of Hand"),
            Skill::Stealth => write!(f, "Stealth"),
            Skill::Arcana => write!(f, "Arcana"),
            Skill::History => write!(f, "History"),
            Skill::Investigation => write!(f, "Investigation"),
            Skill::Nature => write!(f, "Nature"),
            Skill::Religion => write!(f, "Religion"),
            Skill::AnimalHandling => write!(f, "Animal Handling"),
            Skill::Insight => write!(f, "Insight"),
            Skill::Medicine => write!(f, "Medicine"),
            Skill::Perception => write!(f, "Perception"),
            Skill::Survival => write!(f, "Survival"),
            Skill::Deception => write!(f, "Deception"),
            Skill::Intimidation => write!(f, "Intimidation"),
            Skill::Performance => write!(f, "Performance"),
            Skill::Persuasion => write!(f, "Persuasion"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Direction {
    North,
    South,
    East,
    West,
    Up,
    Down,
}

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::North => write!(f, "north"),
            Direction::South => write!(f, "south"),
            Direction::East => write!(f, "east"),
            Direction::West => write!(f, "west"),
            Direction::Up => write!(f, "up"),
            Direction::Down => write!(f, "down"),
        }
    }
}

pub type LocationId = u32;
pub type NpcId = u32;
pub type ItemId = u32;
pub type TriggerId = u32;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ability_modifier() {
        assert_eq!(Ability::modifier(10), 0);
        assert_eq!(Ability::modifier(11), 0);
        assert_eq!(Ability::modifier(12), 1);
        assert_eq!(Ability::modifier(8), -1);
        assert_eq!(Ability::modifier(1), -5);
        assert_eq!(Ability::modifier(20), 5);
    }

    #[test]
    fn test_skill_ability_mapping() {
        assert_eq!(Skill::Athletics.ability(), Ability::Strength);
        assert_eq!(Skill::Stealth.ability(), Ability::Dexterity);
        assert_eq!(Skill::Arcana.ability(), Ability::Intelligence);
        assert_eq!(Skill::Perception.ability(), Ability::Wisdom);
        assert_eq!(Skill::Persuasion.ability(), Ability::Charisma);
    }
}
