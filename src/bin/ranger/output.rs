use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};

/// When to colorize human-readable output.
#[derive(Clone, Copy, Default)]
pub enum ColorChoice {
    /// Colorize when stdout is a terminal (and `NO_COLOR`/`TERM=dumb` are unset).
    #[default]
    Auto,
    /// Always colorize.
    Always,
    /// Never colorize.
    Never,
}

impl std::str::FromStr for ColorChoice {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(ColorChoice::Auto),
            "always" => Ok(ColorChoice::Always),
            "never" => Ok(ColorChoice::Never),
            _ => Err(format!(
                "invalid value '{s}' (expected auto, always, or never)"
            )),
        }
    }
}

static USE_COLOR: AtomicBool = AtomicBool::new(false);

/// Resolve and store the global color decision. Call once at startup, before
/// any output is produced.
pub fn init_color(choice: ColorChoice) {
    let enabled = match choice {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => color_auto(),
    };
    USE_COLOR.store(enabled, Ordering::Relaxed);
}

/// Auto-detection per the conventions in <https://clig.dev>: honor `NO_COLOR`,
/// disable for `TERM=dumb`, and otherwise only colorize a real terminal.
fn color_auto() -> bool {
    if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        return false;
    }
    if std::env::var_os("TERM").is_some_and(|v| v == "dumb") {
        return false;
    }
    std::io::stdout().is_terminal()
}

/// Whether colorized output is currently enabled.
pub fn use_color() -> bool {
    USE_COLOR.load(Ordering::Relaxed)
}

pub fn print<T: Serialize + std::fmt::Debug>(value: &T, json: bool, human: impl FnOnce(&T)) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        human(value);
    }
}

pub fn print_list<T: Serialize + std::fmt::Debug>(
    values: &[T],
    json: bool,
    empty_note: &str,
    human: impl Fn(&T),
) {
    if json {
        println!("{}", serde_json::to_string_pretty(values).unwrap());
    } else if values.is_empty() {
        // Stderr so a pipe still sees an empty stdout; `--json` keeps printing `[]`.
        eprintln!("{empty_note}");
    } else {
        for v in values {
            human(v);
        }
    }
}

/// The outcome of a confirmation prompt.
pub enum Confirm {
    /// Proceed with the action.
    Yes,
    /// The user was prompted and declined.
    No,
    /// Not interactive — the caller should require `--yes`.
    NeedsFlag,
}

/// Prompt on stderr to confirm a destructive action.
///
/// Returns [`Confirm::Yes`]/[`Confirm::No`] from the user's answer when stdin is
/// a terminal. When not interactive, returns [`Confirm::NeedsFlag`] so the caller
/// can tell the user to pass `--yes` (never blocks a script on a prompt it can't
/// answer). Call this only when the user has not already passed `--yes`.
pub fn confirm(prompt: &str) -> Confirm {
    if !std::io::stdin().is_terminal() {
        return Confirm::NeedsFlag;
    }
    eprint!("{prompt} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return Confirm::No;
    }
    let answer = line.trim().to_ascii_lowercase();
    if answer == "y" || answer == "yes" {
        Confirm::Yes
    } else {
        Confirm::No
    }
}

/// The number of key characters to display (matching jj's short change ID style).
const DISPLAY_LEN: usize = 8;

/// Format a key for display: unique prefix in bold, remainder (up to DISPLAY_LEN) in dim.
/// When color is disabled, returns the plain prefix (up to DISPLAY_LEN chars).
pub fn format_key(key: &str, prefix_len: usize) -> String {
    let show = &key[..DISPLAY_LEN.min(key.len())];
    if !use_color() {
        return show.to_string();
    }
    let unique = prefix_len.min(show.len());
    let bold = &show[..unique];
    let dim = &show[unique..];
    format!("\x1b[1m{bold}\x1b[0m\x1b[2m{dim}\x1b[0m")
}

/// Shorthand: format a key when you only have a single key and its prefix length.
pub fn format_key_from_map(key: &str, prefix_lengths: &HashMap<String, usize>) -> String {
    let len = prefix_lengths.get(key).copied().unwrap_or(DISPLAY_LEN);
    format_key(key, len)
}

/// Format a tag name for display (`#name`), cyan when color is enabled.
pub fn format_tag(name: &str) -> String {
    if use_color() {
        format!("\x1b[36m#{name}\x1b[0m")
    } else {
        format!("#{name}")
    }
}
