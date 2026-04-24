use ratatui::{buffer::Buffer, layout::Rect, style::Style};

pub(super) fn patch_preview_highlights(buf: &mut Buffer, area: Rect, query: &str, style: Style) {
    let q_chars: Vec<char> = query.chars().collect();
    if q_chars.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    let case_sensitive = query.chars().any(|c| c.is_uppercase());

    for dy in 0..area.height {
        let row_y = area.y + dy;
        let row_chars: Vec<char> = (0..area.width)
            .map(|dx| {
                buf[(area.x + dx, row_y)]
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();

        for start in find_match_starts(&row_chars, &q_chars, case_sensitive) {
            for k in 0..q_chars.len() {
                let col = area.x + (start + k) as u16;
                if col < area.x + area.width {
                    buf[(col, row_y)].set_style(style);
                }
            }
        }
    }
}

fn find_match_starts(haystack: &[char], needle: &[char], case_sensitive: bool) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        let m = (0..needle.len()).all(|k| chars_match(haystack[i + k], needle[k], case_sensitive));
        if m {
            matches.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    matches
}

fn chars_match(a: char, b: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        a == b
    } else {
        a.to_lowercase().eq(b.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn starts(haystack: &str, query: &str) -> Vec<usize> {
        let h: Vec<char> = haystack.chars().collect();
        let q: Vec<char> = query.chars().collect();
        let case_sensitive = query.chars().any(|c| c.is_uppercase());
        find_match_starts(&h, &q, case_sensitive)
    }

    #[test]
    fn lowercase_query_is_case_insensitive() {
        assert_eq!(starts("Hello World", "hello"), vec![0]);
        assert_eq!(starts("HELLO hello HeLLo", "hello"), vec![0, 6, 12]);
    }

    #[test]
    fn uppercase_query_is_case_sensitive() {
        assert_eq!(starts("Hello hello HELLO", "Hello"), vec![0]);
        assert_eq!(starts("Hello hello HELLO", "HELLO"), vec![12]);
    }

    #[test]
    fn empty_query_yields_no_matches() {
        assert_eq!(starts("anything", ""), Vec::<usize>::new());
    }

    #[test]
    fn query_longer_than_haystack_yields_no_matches() {
        assert_eq!(starts("hi", "hello"), Vec::<usize>::new());
    }

    #[test]
    fn regex_metacharacters_are_literal() {
        assert_eq!(starts("a.b a.b aXb", ".b"), vec![1, 5]);
        assert_eq!(starts("a+b a.b", "+"), vec![1]);
    }

    #[test]
    fn overlapping_matches_are_non_overlapping_greedy() {
        assert_eq!(starts("aaaa", "aa"), vec![0, 2]);
    }

    #[test]
    fn unicode_query_lowercase_is_case_insensitive() {
        assert_eq!(starts("CAFÉ café", "café"), vec![0, 5]);
    }

    fn hl_style() -> Style {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    }

    fn fill_row(buf: &mut Buffer, y: u16, text: &str) {
        buf.set_string(0, y, text, Style::default());
    }

    fn collect_highlighted(buf: &Buffer, area: Rect) -> Vec<(u16, u16)> {
        let mut out = Vec::new();
        for dy in 0..area.height {
            for dx in 0..area.width {
                let x = area.x + dx;
                let y = area.y + dy;
                if buf[(x, y)].bg == Color::Yellow {
                    out.push((x, y));
                }
            }
        }
        out
    }

    #[test]
    fn patch_highlights_single_match_in_row() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "hello world         ");

        patch_preview_highlights(&mut buf, area, "hello", hl_style());

        assert_eq!(
            collect_highlighted(&buf, area),
            vec![(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)]
        );
    }

    #[test]
    fn patch_highlights_multiple_matches_in_row() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "foo bar foo bar foo ");

        patch_preview_highlights(&mut buf, area, "foo", hl_style());

        assert_eq!(
            collect_highlighted(&buf, area),
            vec![
                (0, 0),
                (1, 0),
                (2, 0),
                (8, 0),
                (9, 0),
                (10, 0),
                (16, 0),
                (17, 0),
                (18, 0),
            ]
        );
    }

    #[test]
    fn patch_highlights_across_multiple_rows_independently() {
        let area = Rect::new(0, 0, 10, 3);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "apple     ");
        fill_row(&mut buf, 1, "no match  ");
        fill_row(&mut buf, 2, "  apple   ");

        patch_preview_highlights(&mut buf, area, "apple", hl_style());

        let got = collect_highlighted(&buf, area);
        assert_eq!(
            got,
            vec![
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (2, 2),
                (3, 2),
                (4, 2),
                (5, 2),
                (6, 2),
            ]
        );
    }

    #[test]
    fn patch_leaves_buffer_untouched_when_no_match() {
        let area = Rect::new(0, 0, 20, 1);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "nothing to see here ");

        patch_preview_highlights(&mut buf, area, "xyz", hl_style());

        assert!(collect_highlighted(&buf, area).is_empty());
    }

    #[test]
    fn patch_with_empty_query_is_noop() {
        let area = Rect::new(0, 0, 10, 1);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "helloworld");

        patch_preview_highlights(&mut buf, area, "", hl_style());

        assert!(collect_highlighted(&buf, area).is_empty());
    }

    #[test]
    fn patch_respects_non_zero_origin_area() {
        let full = Rect::new(0, 0, 30, 5);
        let mut buf = Buffer::empty(full);
        fill_row(&mut buf, 2, "                    cat dog       ");
        let inner = Rect::new(20, 2, 7, 1);

        patch_preview_highlights(&mut buf, inner, "cat", hl_style());

        assert_eq!(
            collect_highlighted(&buf, full),
            vec![(20, 2), (21, 2), (22, 2)]
        );
    }

    #[test]
    fn patch_does_not_match_across_row_boundaries() {
        let area = Rect::new(0, 0, 5, 2);
        let mut buf = Buffer::empty(area);
        fill_row(&mut buf, 0, "hel  ");
        fill_row(&mut buf, 1, "lo   ");

        patch_preview_highlights(&mut buf, area, "hello", hl_style());

        assert!(collect_highlighted(&buf, area).is_empty());
    }
}
