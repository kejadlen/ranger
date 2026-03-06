use serde::Serialize;
use std::collections::HashMap;

pub fn print<T: Serialize + std::fmt::Debug>(value: &T, json: bool, human: impl FnOnce(&T)) {
    if json {
        println!("{}", serde_json::to_string_pretty(value).unwrap());
    } else {
        human(value);
    }
}

pub fn print_list<T: Serialize + std::fmt::Debug>(values: &[T], json: bool, human: impl Fn(&T)) {
    if json {
        println!("{}", serde_json::to_string_pretty(values).unwrap());
    } else {
        for v in values {
            human(v);
        }
    }
}

/// The number of key characters to display (matching jj's short change ID style).
const DISPLAY_LEN: usize = 8;

/// Format a key for display: unique prefix in bold, remainder (up to DISPLAY_LEN) in dim.
/// If the terminal doesn't support colors, returns the plain 8-char prefix.
pub fn format_key(key: &str, prefix_len: usize) -> String {
    let show = &key[..DISPLAY_LEN.min(key.len())];
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
