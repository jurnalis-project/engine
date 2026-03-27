use std::io::{self, BufRead, Write};
use jurnalis_engine::{new_game, process_input};

/// Run the REPL loop with injectable I/O for testability.
/// Returns when the user types "quit" or "exit", or when input is exhausted.
fn run_repl<R: BufRead, W: Write>(reader: &mut R, writer: &mut W, seed: u64) -> io::Result<()> {
    let output = new_game(seed);
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
