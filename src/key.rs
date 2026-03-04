use crate::error::RangerError;
use rand::Rng;

/// jj-style alphabet for pronounceable keys.
const ALPHABET: &[char] = &[
    'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
];

const KEY_LENGTH: usize = 16;

pub fn generate_key() -> String {
    let mut rng = rand::rng();
    (0..KEY_LENGTH)
        .map(|_| {
            let idx = rng.random_range(0..ALPHABET.len());
            ALPHABET[idx]
        })
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
