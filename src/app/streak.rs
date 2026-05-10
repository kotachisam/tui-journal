use backend::Entry;
use chrono::{Local, NaiveDate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreakStatus {
    Active(u32),
    Jeopardy(u32),
}

pub fn qualifying_writing_days(entries: &[Entry], min_words: u32) -> Vec<NaiveDate> {
    let mut days: Vec<NaiveDate> = entries
        .iter()
        .filter(|e| entry_qualifies(e, min_words))
        .map(|e| e.date.with_timezone(&Local).date_naive())
        .collect();
    days.sort();
    days.dedup();
    days
}

fn entry_qualifies(entry: &Entry, min_words: u32) -> bool {
    let trimmed = entry.content.trim();
    if min_words == 0 {
        return !trimmed.is_empty();
    }
    trimmed.split_whitespace().count() as u32 >= min_words
}

pub fn compute_streak(writing_days: &[NaiveDate], today: NaiveDate) -> Option<StreakStatus> {
    let last = *writing_days.last()?;
    let gap = today.signed_duration_since(last).num_days();
    if !(0..2).contains(&gap) {
        return None;
    }

    let mut streak: u32 = 1;
    let mut prev = last;
    for &day in writing_days.iter().rev().skip(1) {
        if day == prev {
            continue;
        }
        if prev.signed_duration_since(day).num_days() == 1 {
            streak += 1;
            prev = day;
        } else {
            break;
        }
    }

    if gap == 0 {
        Some(StreakStatus::Active(streak))
    } else {
        Some(StreakStatus::Jeopardy(streak))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate, Utc};

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn entry_on(date_str: &str, content: &str) -> Entry {
        let dt: DateTime<Utc> = format!("{date_str}T12:00:00Z").parse().unwrap();
        Entry::new(0, dt, "t".into(), content.into(), vec![], None)
    }

    #[test]
    fn qualifying_days_filters_below_threshold() {
        let entries = vec![
            entry_on("2026-05-06", "one"),
            entry_on("2026-05-07", "one two three"),
            entry_on("2026-05-08", "  "),
            entry_on("2026-05-09", "four five six seven"),
        ];
        assert_eq!(
            qualifying_writing_days(&entries, 3),
            vec![d("2026-05-07"), d("2026-05-09")]
        );
    }

    #[test]
    fn qualifying_days_min_words_zero_accepts_any_non_whitespace() {
        let entries = vec![
            entry_on("2026-05-06", ""),
            entry_on("2026-05-07", "   \n\t  "),
            entry_on("2026-05-08", "x"),
        ];
        assert_eq!(qualifying_writing_days(&entries, 0), vec![d("2026-05-08")]);
    }

    #[test]
    fn qualifying_days_dedupes_same_day_entries() {
        let entries = vec![
            entry_on("2026-05-08", "alpha bravo charlie"),
            entry_on("2026-05-08", "delta echo foxtrot"),
        ];
        assert_eq!(qualifying_writing_days(&entries, 3), vec![d("2026-05-08")]);
    }

    #[test]
    fn qualifying_days_ignores_title_only_entries() {
        let entries = vec![entry_on("2026-05-08", "")];
        assert_eq!(
            qualifying_writing_days(&entries, 3),
            Vec::<NaiveDate>::new()
        );
    }

    #[test]
    fn empty_input_returns_none() {
        assert_eq!(compute_streak(&[], d("2026-05-08")), None);
    }

    #[test]
    fn only_today_returns_active_one() {
        let days = [d("2026-05-08")];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(1))
        );
    }

    #[test]
    fn only_yesterday_returns_jeopardy_one() {
        let days = [d("2026-05-07")];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Jeopardy(1))
        );
    }

    #[test]
    fn only_two_days_ago_returns_none() {
        let days = [d("2026-05-06")];
        assert_eq!(compute_streak(&days, d("2026-05-08")), None);
    }

    #[test]
    fn five_consecutive_ending_today_active_five() {
        let days = [
            d("2026-05-04"),
            d("2026-05-05"),
            d("2026-05-06"),
            d("2026-05-07"),
            d("2026-05-08"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(5))
        );
    }

    #[test]
    fn five_consecutive_ending_yesterday_jeopardy_five() {
        let days = [
            d("2026-05-03"),
            d("2026-05-04"),
            d("2026-05-05"),
            d("2026-05-06"),
            d("2026-05-07"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Jeopardy(5))
        );
    }

    #[test]
    fn five_consecutive_ending_two_days_ago_none() {
        let days = [
            d("2026-05-02"),
            d("2026-05-03"),
            d("2026-05-04"),
            d("2026-05-05"),
            d("2026-05-06"),
        ];
        assert_eq!(compute_streak(&days, d("2026-05-08")), None);
    }

    #[test]
    fn single_skip_resets_streak_to_one() {
        let days = [
            d("2026-05-05"),
            d("2026-05-06"),
            // skip 2026-05-07
            d("2026-05-08"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(1))
        );
    }

    #[test]
    fn streak_extends_across_no_skips() {
        let days = [
            d("2026-05-05"),
            d("2026-05-06"),
            d("2026-05-07"),
            d("2026-05-08"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(4))
        );
    }

    #[test]
    fn five_in_a_row_then_today_skipped_yields_jeopardy_five() {
        let days = [
            d("2026-05-03"),
            d("2026-05-04"),
            d("2026-05-05"),
            d("2026-05-06"),
            d("2026-05-07"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Jeopardy(5))
        );
    }

    #[test]
    fn five_in_a_row_then_two_days_skipped_yields_none() {
        let days = [
            d("2026-05-02"),
            d("2026-05-03"),
            d("2026-05-04"),
            d("2026-05-05"),
            d("2026-05-06"),
        ];
        assert_eq!(compute_streak(&days, d("2026-05-08")), None);
    }

    #[test]
    fn duplicate_dates_counted_once() {
        let days = [
            d("2026-05-06"),
            d("2026-05-06"),
            d("2026-05-07"),
            d("2026-05-07"),
            d("2026-05-08"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(3))
        );
    }

    #[test]
    fn earlier_break_does_not_extend_recent_streak() {
        let days = [
            d("2026-04-01"),
            d("2026-04-02"),
            // big gap
            d("2026-05-06"),
            d("2026-05-07"),
            d("2026-05-08"),
        ];
        assert_eq!(
            compute_streak(&days, d("2026-05-08")),
            Some(StreakStatus::Active(3))
        );
    }

    #[test]
    fn future_dated_entries_treated_as_no_data() {
        let days = [d("2026-06-01")];
        assert_eq!(compute_streak(&days, d("2026-05-08")), None);
    }
}
