const POOL: &[&str] = &[
    "yoooo. day {streak} :)",
    "{streak} and counting",
    "day {streak}. let's go",
    "back at it on day {streak}",
    "spill it",
    "what's the move today",
    "go ahead",
    "back again?",
    "start anywhere",
    "what happened",
];

const STREAK_TOKEN: &str = "{streak}";
const MIN_STREAK_FOR_TEMPLATE: u32 = 2;

pub fn pick_placeholder(seed: u64, streak: Option<u32>) -> String {
    let streak_eligible = streak.is_some_and(|s| s >= MIN_STREAK_FOR_TEMPLATE);
    let pool: Vec<&&str> = POOL
        .iter()
        .filter(|line| streak_eligible || !line.contains(STREAK_TOKEN))
        .collect();

    let chosen = pool[(seed as usize) % pool.len()];
    match streak {
        Some(s) if chosen.contains(STREAK_TOKEN) => chosen.replace(STREAK_TOKEN, &s.to_string()),
        _ => (*chosen).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_streak_excludes_templated_lines() {
        for seed in 0..200 {
            let line = pick_placeholder(seed, None);
            assert!(
                !line.contains(STREAK_TOKEN),
                "Unsubstituted token leaked: {line}"
            );
            assert!(
                !line.contains("day "),
                "Streak-templated line leaked when streak=None: {line}"
            );
        }
    }

    #[test]
    fn streak_below_threshold_excludes_templated_lines() {
        for seed in 0..200 {
            let line = pick_placeholder(seed, Some(1));
            assert!(!line.contains(STREAK_TOKEN));
            assert!(!line.contains("day "));
        }
    }

    #[test]
    fn streak_at_threshold_can_yield_templated_line() {
        let mut saw_template = false;
        for seed in 0..200 {
            let line = pick_placeholder(seed, Some(2));
            if line.contains("day 2") || line.contains("2 and counting") {
                saw_template = true;
                break;
            }
        }
        assert!(saw_template, "Streak-templated lines were never selected");
    }

    #[test]
    fn streak_value_is_substituted() {
        for seed in 0..200 {
            let line = pick_placeholder(seed, Some(21));
            assert!(!line.contains(STREAK_TOKEN));
            if line.contains("day ") || line.contains(" and counting") {
                assert!(
                    line.contains("21"),
                    "Streak value missing from templated line: {line}"
                );
            }
        }
    }

    #[test]
    fn deterministic_for_seed() {
        let a = pick_placeholder(42, Some(10));
        let b = pick_placeholder(42, Some(10));
        assert_eq!(a, b);
    }

    #[test]
    fn always_returns_non_empty() {
        for seed in 0..50 {
            assert!(!pick_placeholder(seed, None).is_empty());
            assert!(!pick_placeholder(seed, Some(0)).is_empty());
            assert!(!pick_placeholder(seed, Some(7)).is_empty());
        }
    }
}
