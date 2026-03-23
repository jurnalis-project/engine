use rand::Rng;

pub fn roll_d20(rng: &mut impl Rng) -> i32 {
    rng.gen_range(1..=20)
}

pub fn roll_dice(rng: &mut impl Rng, count: u32, sides: u32) -> Vec<i32> {
    (0..count).map(|_| rng.gen_range(1..=sides as i32)).collect()
}

pub fn roll_4d6_drop_lowest(rng: &mut impl Rng) -> i32 {
    let mut rolls = roll_dice(rng, 4, 6);
    rolls.sort();
    rolls[1..].iter().sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn test_roll_d20_range() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let roll = roll_d20(&mut rng);
            assert!(roll >= 1 && roll <= 20, "d20 roll {} out of range", roll);
        }
    }

    #[test]
    fn test_roll_d20_deterministic() {
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        for _ in 0..10 {
            assert_eq!(roll_d20(&mut rng1), roll_d20(&mut rng2));
        }
    }

    #[test]
    fn test_roll_4d6_drop_lowest_range() {
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let result = roll_4d6_drop_lowest(&mut rng);
            assert!(result >= 3 && result <= 18, "4d6kh3 result {} out of range", result);
        }
    }

    #[test]
    fn test_roll_dice_count() {
        let mut rng = StdRng::seed_from_u64(42);
        let rolls = roll_dice(&mut rng, 3, 8);
        assert_eq!(rolls.len(), 3);
        for roll in rolls {
            assert!(roll >= 1 && roll <= 8);
        }
    }
}
