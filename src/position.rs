/// Generate a position string between `before` and `after`.
/// Both are optional: None means "the boundary" (start or end).
/// Uses base-26 (a-z) characters with lexicographic ordering.
pub fn midpoint(before: Option<&str>, after: Option<&str>) -> String {
    match (before, after) {
        (None, None) => "m".to_string(),
        (None, Some(b)) => generate_between("", b),
        (Some(a), None) => generate_after(a),
        (Some(a), Some(b)) => {
            debug_assert!(a < b, "before ({a}) must be less than after ({b})");
            generate_between(a, b)
        }
    }
}

fn generate_after(a: &str) -> String {
    let mut digits: Vec<u8> = a.bytes().map(|c| c - b'a').collect();

    // Find the rightmost character with room to increment toward 'z'
    for i in (0..digits.len()).rev() {
        let mid = (digits[i] as u16 + 25) / 2;
        if mid as u8 > digits[i] {
            digits[i] = mid as u8;
            digits.truncate(i + 1);
            return digits.iter().map(|&d| (d + b'a') as char).collect();
        }
    }

    // All characters near 'z'; extend with 'm'
    format!("{a}m")
}

fn generate_between(a: &str, b: &str) -> String {
    let a_digits: Vec<u8> = a.bytes().map(|c| c - b'a').collect();
    let b_digits: Vec<u8> = b.bytes().map(|c| c - b'a').collect();

    let mut result: Vec<u8> = Vec::new();
    let max_len = a_digits.len().max(b_digits.len());

    for i in 0..=max_len {
        let ca = a_digits.get(i).copied().unwrap_or(0);
        let cb = b_digits.get(i).copied().unwrap_or(25);

        if ca == cb {
            result.push(ca);
            continue;
        }

        // ca < cb
        let mid = (ca as u16 + cb as u16) / 2;
        if mid as u8 > ca {
            result.push(mid as u8);
            return result.iter().map(|&d| (d + b'a') as char).collect();
        }

        // Adjacent (differ by 1): take the lower, then find suffix between
        // remaining digits of a and 'z' (implicit upper bound)
        result.push(ca);
        for j in (i + 1)..=(max_len + 16) {
            let da = a_digits.get(j).copied().unwrap_or(0);
            let mid2 = (da as u16 + 25) / 2;
            if mid2 as u8 > da {
                result.push(mid2 as u8);
                return result.iter().map(|&d| (d + b'a') as char).collect();
            }
            result.push(da);
        }
    }

    // Fallback
    result.push(12);
    result.iter().map(|&d| (d + b'a') as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_position() {
        let pos = midpoint(None, None);
        assert_eq!(pos, "m");
    }

    #[test]
    fn before_existing() {
        let pos = midpoint(None, Some("m"));
        assert!(*pos < *"m", "expected {pos} < m");
    }

    #[test]
    fn after_existing() {
        let pos = midpoint(Some("m"), None);
        assert!(*pos > *"m", "expected {pos} > m");
    }

    #[test]
    fn between_two() {
        let pos = midpoint(Some("a"), Some("z"));
        assert!(*pos > *"a", "expected {pos} > a");
        assert!(*pos < *"z", "expected {pos} < z");
    }

    #[test]
    fn between_adjacent() {
        let pos = midpoint(Some("a"), Some("b"));
        assert!(*pos > *"a", "expected {pos} > a");
        assert!(*pos < *"b", "expected {pos} < b");
    }

    #[test]
    fn ordering_is_stable_over_many_appends() {
        let mut positions = vec![midpoint(None, None)];
        for _ in 0..20 {
            let last = positions.last().unwrap().clone();
            positions.push(midpoint(Some(&last), None));
        }
        for window in positions.windows(2) {
            assert!(
                window[0] < window[1],
                "{} should be < {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn ordering_is_stable_over_many_prepends() {
        let mut positions = vec![midpoint(None, None)];
        for _ in 0..20 {
            let first = positions.first().unwrap().clone();
            positions.insert(0, midpoint(None, Some(&first)));
        }
        for window in positions.windows(2) {
            assert!(
                window[0] < window[1],
                "{} should be < {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn ordering_is_stable_over_many_interleaved_inserts() {
        // Build a list by always inserting in the middle
        let mut positions = vec![
            midpoint(None, None), // first
        ];
        positions.push(midpoint(Some(&positions[0]), None)); // second

        for _ in 0..20 {
            let a = &positions[positions.len() - 2].clone();
            let b = &positions[positions.len() - 1].clone();
            let mid = midpoint(Some(a), Some(b));
            assert!(mid > *a, "{mid} should be > {a}");
            assert!(mid < *b, "{mid} should be < {b}");
            positions.insert(positions.len() - 1, mid);
        }
    }
}
