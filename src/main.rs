use std::io::{self, BufRead, Write};
use jurnalis_engine::{new_game, process_input};

fn sanitize_save_name(raw: Option<&str>) -> String {
    let name = raw.unwrap_or("autosave").trim();
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid { name.to_string() } else { "autosave".to_string() }
}

fn handle_cli_persistence_command(
    input: &str,
    state_json: &mut String,
    save_dir: &std::path::Path,
) -> io::Result<Option<Vec<String>>> {
    let trimmed = input.trim();
    let parts: Vec<&str> = trimmed.split_whitespace().collect();
    if parts.is_empty() {
        return Ok(None);
    }

    let verb = parts[0].to_ascii_lowercase();
    let arg = parts.get(1).copied();

    match verb.as_str() {
        "save" => {
            std::fs::create_dir_all(save_dir)?;
            let slot = sanitize_save_name(arg);
            let path = save_dir.join(format!("{}.json", slot));
            std::fs::write(&path, state_json.as_bytes())?;
            Ok(Some(vec![format!("Saved game to {}.", path.file_name().unwrap().to_string_lossy())]))
        }
        "load" | "restore" => {
            let slot = sanitize_save_name(arg);
            let path = save_dir.join(format!("{}.json", slot));
            let loaded = std::fs::read_to_string(&path).map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("No save file named '{}' was found.", slot),
                    )
                } else {
                    e
                }
            })?;
            // Validate shape/version through engine state loader.
            jurnalis_engine::state::load_game(&loaded)
                .map_err(|msg| io::Error::new(io::ErrorKind::InvalidData, msg))?;
            *state_json = loaded;
            Ok(Some(vec![format!("Loaded game from {}.", path.file_name().unwrap().to_string_lossy())]))
        }
        _ => Ok(None),
    }
}

/// Run the REPL loop with injectable I/O for testability.
/// Returns when the user types "quit" or "exit", or when input is exhausted.
fn run_repl<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, seed: u64) -> io::Result<()> {
    let save_dir = std::path::PathBuf::from("saves");
    run_repl_with_save_dir(reader, writer, seed, &save_dir)
}

/// Run the REPL starting from an already-loaded state JSON (dev mode entry point).
#[cfg(feature = "dev")]
fn run_repl_from_state<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    initial_state_json: String,
    save_dir: &std::path::Path,
) -> io::Result<()> {
    let mut state_json = initial_state_json;
    run_repl_loop(reader, writer, &mut state_json, save_dir)
}

/// Core REPL loop shared by normal and dev-mode entry points.
fn run_repl_loop<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    state_json: &mut String,
    save_dir: &std::path::Path,
) -> io::Result<()> {
    let mut input_line = String::new();
    loop {
        write!(writer, "> ")?;
        writer.flush()?;

        input_line.clear();
        let bytes_read = reader.read_line(&mut input_line)?;
        if bytes_read == 0 {
            break;
        }

        let trimmed = input_line.trim();
        if trimmed.eq_ignore_ascii_case("quit") || trimmed.eq_ignore_ascii_case("exit") {
            writeln!(writer, "Farewell, adventurer.")?;
            break;
        }

        match handle_cli_persistence_command(trimmed, state_json, save_dir) {
            Ok(Some(lines)) => {
                for line in &lines {
                    writeln!(writer, "{}", line)?;
                }
                let lower = trimmed.to_ascii_lowercase();
                if lower == "load" || lower.starts_with("load ")
                    || lower == "restore" || lower.starts_with("restore ")
                {
                    let look = process_input(state_json, "look");
                    for line in &look.text {
                        writeln!(writer, "{}", line)?;
                    }
                    *state_json = look.state_json;
                }
                continue;
            }
            Ok(None) => {}
            Err(e) => {
                writeln!(writer, "Error: {}", e)?;
                continue;
            }
        }

        let output = process_input(state_json, trimmed);
        for line in &output.text {
            writeln!(writer, "{}", line)?;
        }
        *state_json = output.state_json;
    }
    Ok(())
}

/// Run the REPL with a specified save directory for persistence.
fn run_repl_with_save_dir<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    seed: u64,
    save_dir: &std::path::Path,
) -> io::Result<()> {
    let output = new_game(seed, false);
    for line in &output.text {
        writeln!(writer, "{}", line)?;
    }
    let mut state_json = output.state_json;
    run_repl_loop(reader, writer, &mut state_json, save_dir)
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

    // Dev mode: --dev-state <file> injects an arbitrary pre-crafted GameState.
    #[cfg(feature = "dev")]
    {
        let args: Vec<String> = std::env::args().collect();
        if let Some(pos) = args.iter().position(|a| a == "--dev-state") {
            if let Some(path) = args.get(pos + 1) {
                let state_json = match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error reading dev-state file '{}': {}", path, e);
                        std::process::exit(1);
                    }
                };
                let output = jurnalis_engine::new_game_from_state(&state_json);
                for line in &output.text {
                    writeln!(writer, "{}", line).ok();
                }
                if output.state_json.is_empty() {
                    std::process::exit(1);
                }
                let save_dir = std::path::PathBuf::from("saves");
                if let Err(e) = run_repl_from_state(&mut reader, &mut writer, output.state_json, &save_dir) {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                return;
            } else {
                eprintln!("--dev-state requires a file path argument");
                std::process::exit(1);
            }
        }
    }

    if let Err(e) = run_repl(&mut reader, &mut writer, seed) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

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

    // ---- Bug #90: load nonexistent returns raw OS error ----
    #[test]
    fn load_nonexistent_file_returns_friendly_error() {
        // Hypothesis: std::fs::read_to_string returns an OS-level "No such
        // file or directory" error which is printed raw. Fix: intercept
        // NotFound and replace with a human-readable message.
        let tmp = std::env::temp_dir().join(format!("jurnalis_cli_nonexistent_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();

        let mut state_json = new_game(42, false).state_json;
        let result = handle_cli_persistence_command("load nonexistent", &mut state_json, &tmp);

        match result {
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("No save file named") || msg.contains("nonexistent"),
                    "Expected friendly error message, got: {}", msg
                );
                assert!(
                    !msg.to_lowercase().contains("os error") && !msg.contains("(os error"),
                    "Error should not expose raw OS error, got: {}", msg
                );
            }
            Ok(_) => panic!("Expected error for nonexistent save, got Ok"),
        }

        std::fs::remove_dir_all(&tmp).ok();
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
