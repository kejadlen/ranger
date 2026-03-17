/// Generate a position string lexicographically between `a` and `b`.
/// Uses base-26 (a-z) characters. Empty `a` means "beginning" (digits
/// default to 0), empty `b` means "end" (digits default to 25/'z').
pub fn between(a: &str, b: &str) -> String {
    debug_assert!(b.is_empty() || a < b, "a ({a}) must be less than b ({b})");

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

    // Structurally unreachable: the inner loop always terminates via the
    // mid2 > da check within 16 extra iterations (base-26 guarantees room
    // between any digit and 'z').
    unreachable!("between exhausted without finding a midpoint") // cov-excl-line
}

/// Generate `n` well-spaced position strings via recursive bisection.
pub fn spread(n: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(n);
    fill("", "", n, &mut out);
    out
}

fn fill(lo: &str, hi: &str, n: usize, out: &mut Vec<String>) {
    if n == 0 {
        return;
    }
    let mid = n / 2;
    let pos = between(lo, hi);
    fill(lo, &pos, mid, out);
    out.push(pos.clone());
    fill(&pos, hi, n - mid - 1, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn between_empty_bounds() {
        assert_eq!(between("", ""), "m");
    }

    #[test]
    fn before_existing() {
        let pos = between("", "m");
        assert!(*pos < *"m", "expected {pos} < m");
    }

    #[test]
    fn after_existing() {
        let pos = between("m", "");
        assert!(*pos > *"m", "expected {pos} > m");
    }

    #[test]
    fn between_two() {
        let pos = between("a", "z");
        assert!(*pos > *"a", "expected {pos} > a");
        assert!(*pos < *"z", "expected {pos} < z");
    }

    #[test]
    fn between_shared_prefix() {
        let pos = between("ma", "mz");
        assert!(*pos > *"ma", "expected {pos} > ma");
        assert!(*pos < *"mz", "expected {pos} < mz");
    }

    #[test]
    fn between_adjacent() {
        let pos = between("a", "b");
        assert!(*pos > *"a", "expected {pos} > a");
        assert!(*pos < *"b", "expected {pos} < b");
    }

    #[test]
    fn ordering_is_stable_over_many_appends() {
        let mut positions = vec!["m".to_string()];
        for _ in 0..20 {
            let last = positions.last().unwrap().clone();
            positions.push(between(&last, ""));
        }
        for window in positions.windows(2) {
            assert!(window[0] < window[1]);
        }
    }

    #[test]
    fn ordering_is_stable_over_many_prepends() {
        let mut positions = vec!["m".to_string()];
        for _ in 0..20 {
            let first = positions.first().unwrap().clone();
            positions.insert(0, between("", &first));
        }
        for window in positions.windows(2) {
            assert!(window[0] < window[1]);
        }
    }

    #[test]
    fn ordering_is_stable_over_many_interleaved_inserts() {
        let mut positions = vec!["m".to_string()];
        positions.push(between(&positions[0], ""));

        for _ in 0..20 {
            let a = &positions[positions.len() - 2].clone();
            let b = &positions[positions.len() - 1].clone();
            let mid = between(a, b);
            assert!(mid > *a);
            assert!(mid < *b);
            positions.insert(positions.len() - 1, mid);
        }
    }

    #[test]
    fn spread_empty() {
        assert!(spread(0).is_empty());
    }

    #[test]
    fn spread_one() {
        let positions = spread(1);
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0], "m");
    }

    #[test]
    fn spread_produces_sorted_positions() {
        for n in [2, 3, 5, 10, 50, 100] {
            let positions = spread(n);
            assert_eq!(positions.len(), n);
            for window in positions.windows(2) {
                assert!(
                    window[0] < window[1],
                    "not sorted at n={n}: {:?}",
                    positions
                );
            }
        }
    }
}
