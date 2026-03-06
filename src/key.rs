use crate::error::RangerError;
use rand::Rng;

/// jj-style alphabet for pronounceable keys.
const ALPHABET: &[char] = &[
    'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

const KEY_LENGTH: usize = 16;

pub fn generate_key() -> String {
    let mut rng = rand::thread_rng();
    (0..KEY_LENGTH)
        .map(|_| {
            let idx = rng.gen_range(0..ALPHABET.len());
            ALPHABET[idx]
        })
        .collect()
}

/// Returns the minimum prefix length needed to uniquely identify `key` among `all_keys`.
/// The minimum returned value is 1 (even if the set has only one key).
pub fn shortest_unique_prefix_len(key: &str, all_keys: &[String]) -> usize {
    let mut len = 1;
    while len < key.len() {
        let prefix = &key[..len];
        let count = all_keys.iter().filter(|k| k.starts_with(prefix)).count();
        if count <= 1 {
            return len;
        }
        len += 1;
    }
    unreachable!("all keys are the same length") // cov-excl-line
}

/// Builds a map from key → shortest unique prefix length for all keys in the set.
pub fn unique_prefix_lengths(keys: &[String]) -> std::collections::HashMap<String, usize> {
    keys.iter()
        .map(|k| (k.clone(), shortest_unique_prefix_len(k, keys)))
        .collect()
}

pub fn resolve_prefix(prefix: &str, keys: &[String]) -> Result<String, RangerError> {
    let matches: Vec<&String> = keys.iter().filter(|k| k.starts_with(prefix)).collect();
    match matches.len() {
        0 => Err(RangerError::KeyNotFound(prefix.to_string())),
        1 => Ok(matches[0].clone()),
        _ => Err(RangerError::AmbiguousPrefix(prefix.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_key_has_correct_length() {
        let key = generate_key();
        assert_eq!(key.len(), KEY_LENGTH);
    }

    #[test]
    fn generated_key_uses_valid_alphabet() {
        let key = generate_key();
        for ch in key.chars() {
            assert!(
                ALPHABET.contains(&ch),
                "key contains invalid character: {ch}"
            );
        }
    }

    #[test]
    fn generated_keys_are_unique() {
        let keys: Vec<String> = (0..100).map(|_| generate_key()).collect();
        let unique: std::collections::HashSet<&String> = keys.iter().collect();
        assert_eq!(keys.len(), unique.len());
    }

    #[test]
    fn shortest_unique_prefix_single_key() {
        let keys = vec!["romoqtuw".to_string()];
        assert_eq!(shortest_unique_prefix_len("romoqtuw", &keys), 1);
    }

    #[test]
    fn shortest_unique_prefix_diverges_at_second_char() {
        let keys = vec!["romoqtuw".to_string(), "rypqxnkl".to_string()];
        // Both start with 'r', diverge at char 2
        assert_eq!(shortest_unique_prefix_len("romoqtuw", &keys), 2);
        assert_eq!(shortest_unique_prefix_len("rypqxnkl", &keys), 2);
    }

    #[test]
    fn shortest_unique_prefix_longer_shared() {
        let keys = vec![
            "romoqtuw".to_string(),
            "romxnklp".to_string(),
            "rypqxnkl".to_string(),
        ];
        // "romo" vs "romx" need 4 chars, "ry" needs 2
        assert_eq!(shortest_unique_prefix_len("romoqtuw", &keys), 4);
        assert_eq!(shortest_unique_prefix_len("romxnklp", &keys), 4);
        assert_eq!(shortest_unique_prefix_len("rypqxnkl", &keys), 2);
    }

    #[test]
    fn unique_prefix_lengths_builds_map() {
        let keys = vec![
            "romoqtuw".to_string(),
            "rypqxnkl".to_string(),
            "slmnopqr".to_string(),
        ];
        let map = unique_prefix_lengths(&keys);
        assert_eq!(map["romoqtuw"], 2);
        assert_eq!(map["rypqxnkl"], 2);
        assert_eq!(map["slmnopqr"], 1);
    }

    #[test]
    fn resolve_prefix_exact_match() {
        let keys = vec!["romoqtuw".to_string(), "rypqxnkl".to_string()];
        assert_eq!(resolve_prefix("rom", &keys).unwrap(), "romoqtuw");
    }

    #[test]
    fn resolve_prefix_ambiguous() {
        let keys = vec!["romoqtuw".to_string(), "romxnklp".to_string()];
        assert!(resolve_prefix("rom", &keys).is_err());
    }

    #[test]
    fn resolve_prefix_no_match() {
        let keys = vec!["romoqtuw".to_string()];
        assert!(resolve_prefix("xyz", &keys).is_err());
    }
}
