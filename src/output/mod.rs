use serde::{Deserialize, Serialize};

/// Format a dice roll as `"roll+modifier=total"` with correct sign handling.
///
/// When `modifier` is negative the `+` prefix is omitted so the string reads
/// `"13-1=12"` rather than the erroneous `"13+-1=12"`.
pub fn format_roll(roll: i32, modifier: i32, total: i32) -> String {
    if modifier < 0 {
        format!("{}{}={}", roll, modifier, total)
    } else {
        format!("{}+{}={}", roll, modifier, total)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameOutput {
    pub text: Vec<String>,
    pub state_json: String,
    pub state_changed: bool,
}

impl GameOutput {
    pub fn new(text: Vec<String>, state_json: String, state_changed: bool) -> Self {
        Self { text, state_json, state_changed }
    }

    pub fn message(msg: impl Into<String>, state_json: String) -> Self {
        Self {
            text: vec![msg.into()],
            state_json,
            state_changed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_roll_positive_modifier() {
        assert_eq!(format_roll(13, 3, 16), "13+3=16");
    }

    #[test]
    fn test_format_roll_zero_modifier() {
        assert_eq!(format_roll(10, 0, 10), "10+0=10");
    }

    #[test]
    fn test_format_roll_negative_modifier_no_plus() {
        // Bug #91: negative modifier should NOT produce "13+-1=12"
        assert_eq!(format_roll(13, -1, 12), "13-1=12");
        assert_ne!(format_roll(13, -1, 12), "13+-1=12");
    }
}
