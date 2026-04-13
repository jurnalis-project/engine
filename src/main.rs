use std::io::{self, BufRead, Write};
use jurnalis_engine::{new_game, process_input};

/// Run the REPL loop with injectable I/O for testability.
/// Returns when the user types "quit" or "exit", or when input is exhausted.
fn run_repl<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, seed: u64) -> io::Result<()> {
    let output = new_game(seed, false);
    for line in &output.text {
        writeln!(writer, "{}", line)?;
    }
    let mut state_json = output.state_json;

    let mut input_line = String::new();
    loop {
        write!(writer, "> ")?;
        writer.flush()?;

        input_line.clear();
        let bytes_read = reader.read_line(&mut input_line)?;
        if bytes_read == 0 {
            // EOF
            break;
        }

        let trimmed = input_line.trim();
        if trimmed.eq_ignore_ascii_case("quit") || trimmed.eq_ignore_ascii_case("exit") {
            writeln!(writer, "Farewell, adventurer.")?;
            break;
        }

        let output = process_input(&state_json, trimmed);
        for line in &output.text {
            writeln!(writer, "{}", line)?;
        }
        state_json = output.state_json;
    }

    Ok(())
}

fn main() {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(42);

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = stdin.lock();
    let mut writer = stdout.lock();

    if let Err(e) = run_repl(&mut reader, &mut writer, seed) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::path::Path;

    #[test]
    fn save_and_load_commands_round_trip_state_in_repl() {
        let tmp = std::env::temp_dir().join(format!("jurnalis_cli_save_load_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let input = b"1\n1\n1\n15 14 13 12 10 8\n1 2\nAria\nsave slot1\nwest\nload slot1\nlook\nquit\n";
        let mut reader = Cursor::new(&input[..]);
        let mut output = Vec::new();

        run_repl_with_save_dir(&mut reader, &mut output, 42, &tmp).unwrap();

        let out = String::from_utf8(output).unwrap();
        assert!(out.contains("Saved game to slot1.json"), "Output: {}", out);
        assert!(out.contains("Loaded game from slot1.json"), "Output: {}", out);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn load_command_can_replace_defeated_state_before_engine_rejects_input() {
        let tmp = std::env::temp_dir().join(format!("jurnalis_cli_defeat_load_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let healthy = new_game(42, false).state_json;
        std::fs::write(tmp.join("autosave.json"), &healthy).unwrap();

        let mut defeated: jurnalis_engine::state::GameState = serde_json::from_str(&healthy).unwrap();
        defeated.character.current_hp = 0;
        let mut state_json = serde_json::to_string(&defeated).unwrap();

        let lines = handle_cli_persistence_command("load", &mut state_json, &tmp)
            .unwrap()
            .expect("load should be intercepted by CLI");

        assert!(lines.iter().any(|l| l.contains("Loaded game from autosave.json")));

        let reloaded: jurnalis_engine::state::GameState = serde_json::from_str(&state_json).unwrap();
        assert!(reloaded.character.current_hp > 0);

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn quit_exits_repl() {
        let input = b"quit\n";
        let mut reader = Cursor::new(&input[..]);
        let mut output = Vec::new();

        run_repl(&mut reader, &mut output, 42).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Farewell, adventurer."));
        assert!(output_str.contains("> "));
    }

    #[test]
    fn exit_exits_repl() {
        let input = b"exit\n";
        let mut reader = Cursor::new(&input[..]);
        let mut output = Vec::new();

        run_repl(&mut reader, &mut output, 42).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Farewell, adventurer."));
    }

    #[test]
    fn eof_exits_repl() {
        let input = b"";
        let mut reader = Cursor::new(&input[..]);
        let mut output = Vec::new();

        run_repl(&mut reader, &mut output, 42).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        // Should have printed initial game output but no farewell
        assert!(!output_str.contains("Farewell, adventurer."));
        assert!(!output_str.is_empty());
    }

    #[test]
    fn game_input_forwarded_to_engine() {
        // Send "human" (a valid character creation input) then quit
        let input = b"human\nquit\n";
        let mut reader = Cursor::new(&input[..]);
        let mut output = Vec::new();

        run_repl(&mut reader, &mut output, 42).unwrap();

        let output_str = String::from_utf8(output).unwrap();
        // Should contain at least two prompts (one after initial output, one after "human")
        let prompt_count = output_str.matches("> ").count();
        assert!(prompt_count >= 2, "Expected at least 2 prompts, got {}", prompt_count);
    }
}
