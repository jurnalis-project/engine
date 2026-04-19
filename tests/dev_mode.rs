/// Integration tests for dev-mode state injection (`new_game_from_state`).
/// Only compiled when the `dev` Cargo feature is enabled.
#[cfg(feature = "dev")]
mod dev_mode_tests {
    use jurnalis_engine::{new_game_from_state, process_input};
    use jurnalis_engine::state::{GameState, GamePhase};

    fn fixture_path(name: &str) -> std::path::PathBuf {
        // CARGO_MANIFEST_DIR is the jurnalis-engine/ crate dir; fixtures are
        // one level up at the workspace root.
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".into());
        std::path::Path::new(&manifest).join("..").join("fixtures").join(name)
    }

    /// Load valid fixture JSON and verify the output banner and state are returned correctly.
    #[test]
    fn new_game_from_state_valid_json_returns_dev_banner() {
        let fixture = std::fs::read_to_string(fixture_path("post-short-rest.json"))
            .expect("fixtures/post-short-rest.json must exist");

        let output = new_game_from_state(&fixture);

        assert!(!output.state_json.is_empty(), "state_json must not be empty on success");
        assert!(output.text.iter().any(|l| l.contains("DEV MODE")),
            "Output should contain DEV MODE banner, got: {:?}", output.text);
    }

    /// Verify invalid JSON returns an error banner and empty state_json.
    #[test]
    fn new_game_from_state_invalid_json_returns_error() {
        let output = new_game_from_state("not valid json {{{");

        assert!(output.state_json.is_empty(), "state_json must be empty on failure");
        assert!(output.text.iter().any(|l| l.contains("DEV MODE ERROR")),
            "Output should contain DEV MODE ERROR, got: {:?}", output.text);
    }

    /// Verify that the loaded state can be round-tripped through process_input.
    #[test]
    fn new_game_from_state_loaded_state_accepts_input() {
        let fixture = std::fs::read_to_string(fixture_path("post-short-rest.json"))
            .expect("fixtures/post-short-rest.json must exist");

        let output = new_game_from_state(&fixture);
        assert!(!output.state_json.is_empty());

        // Should be able to process a "look" command without errors
        let look_output = process_input(&output.state_json, "look");
        assert!(!look_output.text.is_empty(), "look command should return text");
    }

    /// Verify the fixture for combat scenario is loadable.
    #[test]
    fn combat_fixture_is_valid_and_loadable() {
        let fixture = std::fs::read_to_string(fixture_path("combat-fighter-vs-goblin.json"))
            .expect("fixtures/combat-fighter-vs-goblin.json must exist");

        let output = new_game_from_state(&fixture);

        assert!(!output.state_json.is_empty());
        let state: GameState = serde_json::from_str(&output.state_json)
            .expect("state_json should deserialize");
        assert_eq!(state.character.class.to_string(), "Fighter",
            "combat fixture should be a Fighter");
    }

    /// Verify game_phase is preserved (not reset to CharacterCreation).
    #[test]
    fn new_game_from_state_preserves_game_phase() {
        let fixture = std::fs::read_to_string(fixture_path("post-short-rest.json"))
            .expect("fixtures/post-short-rest.json must exist");

        let output = new_game_from_state(&fixture);

        let state: GameState = serde_json::from_str(&output.state_json)
            .expect("state_json should deserialize");
        assert!(
            matches!(state.game_phase, GamePhase::Exploration),
            "Phase should be preserved as Exploration, got {:?}", state.game_phase
        );
    }
}
