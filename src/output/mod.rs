use serde::{Deserialize, Serialize};

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
