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
    use hegel::TestCase;
    use hegel::generators as gs;

    /// A position as `between` produces it: one or more `a`-`z` digits, never
    /// ending in `a`. A trailing `a` would leave no room below the string
    /// (nothing sorts between `m` and `ma`), and `between` never emits one.
    fn positions() -> impl hegel::PrintableGenerator<String> {
        gs::from_regex("[a-z]{0,8}[b-z]")
    }

    fn assert_valid_position(pos: &str) {
        assert!(!pos.is_empty(), "empty position");
        assert!(
            pos.bytes().all(|c| c.is_ascii_lowercase()),
            "{pos:?} has a non a-z digit"
        );
        assert!(!pos.ends_with('a'), "{pos:?} ends in 'a'");
    }

    #[hegel::test]
    fn prop_between_two_positions_is_strictly_between(tc: TestCase) {
        let x = tc.draw(positions());
        let y = tc.draw(positions());
        tc.assume(x != y);
        let (lo, hi) = if x < y { (x, y) } else { (y, x) };

        let mid = between(&lo, &hi);
        assert_valid_position(&mid);
        assert!(lo < mid && mid < hi, "{lo:?} < {mid:?} < {hi:?}");
    }

    #[hegel::test]
    fn prop_between_open_bounds(tc: TestCase) {
        let pos = tc.draw(positions());

        let after = between(&pos, "");
        assert_valid_position(&after);
        assert!(after > pos, "{after:?} > {pos:?}");

        let before = between("", &pos);
        assert_valid_position(&before);
        assert!(before < pos, "{before:?} < {pos:?}");
    }

    #[hegel::test]
    fn prop_repeated_inserts_stay_sorted(tc: TestCase) {
        // Each step inserts a new position into a random gap (including the
        // open ends) of a growing list, as moving tasks around does.
        let mut list: Vec<String> = Vec::new();
        let steps = tc.draw(gs::integers::<usize>().max_value(40));
        for _ in 0..steps {
            let gap = tc.draw(gs::integers::<usize>().max_value(list.len()));
            let lo = if gap == 0 { "" } else { list[gap - 1].as_str() };
            let hi = list.get(gap).map_or("", String::as_str);
            let pos = between(lo, hi);
            assert_valid_position(&pos);
            list.insert(gap, pos);
        }
        assert!(list.windows(2).all(|w| w[0] < w[1]), "{list:?}");
    }

    #[hegel::test]
    fn prop_spread_is_sorted_and_sized(tc: TestCase) {
        let n = tc.draw(gs::integers::<usize>().max_value(500));
        let out = spread(n);
        assert_eq!(out.len(), n);
        for pos in &out {
            assert_valid_position(pos);
        }
        assert!(out.windows(2).all(|w| w[0] < w[1]), "{out:?}");
    }

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
