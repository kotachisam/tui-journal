use backend::Entry;

use crate::settings::DateFormat;

use super::parsing::{DocMention, parse_mentions_in_doc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedMention {
    pub line_idx: usize,
    pub source_char_range: std::ops::Range<usize>,
    pub label_char_range: std::ops::Range<usize>,
    pub id: u32,
    pub label: String,
    pub missing: bool,
    pub anchor: Option<String>,
}

pub fn render_mention_label(entry: &Entry, date_format: &DateFormat) -> String {
    let title = entry.title.trim();
    let date_str = date_format.display(&entry.date);
    if title.is_empty() {
        date_str
    } else {
        format!("{title} ({date_str})")
    }
}

pub fn substitute_mentions(
    content: &str,
    entries: &[Entry],
    date_format: &DateFormat,
) -> (String, Vec<RenderedMention>) {
    let doc_mentions = parse_mentions_in_doc(content);
    if doc_mentions.is_empty() {
        return (content.to_string(), Vec::new());
    }

    let mut by_line: std::collections::BTreeMap<usize, Vec<&DocMention>> =
        std::collections::BTreeMap::new();
    for m in &doc_mentions {
        by_line.entry(m.line_idx).or_default().push(m);
    }

    let mut rendered_lines: Vec<String> = Vec::new();
    let mut rendered_mentions: Vec<RenderedMention> = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let Some(mentions) = by_line.get(&line_idx) else {
            rendered_lines.push(line.to_string());
            continue;
        };
        let chars: Vec<char> = line.chars().collect();
        let mut new_line = String::new();
        let mut cursor: usize = 0;
        let mut new_char_pos: usize = 0;
        for m in mentions {
            let pre: String = chars[cursor..m.char_range.start].iter().collect();
            new_line.push_str(&pre);
            new_char_pos += m.char_range.start - cursor;

            let entry = entries
                .iter()
                .find(|e| e.id == m.id && e.deleted_at.is_none());
            let (label, missing) = match entry {
                Some(e) => (render_mention_label(e, date_format), false),
                None => {
                    let raw: String = chars[m.char_range.clone()].iter().collect();
                    (raw, true)
                }
            };
            let label_char_count = label.chars().count();
            let label_char_start = new_char_pos;
            new_line.push_str(&label);
            new_char_pos += label_char_count;

            rendered_mentions.push(RenderedMention {
                line_idx,
                source_char_range: m.char_range.clone(),
                label_char_range: label_char_start..label_char_start + label_char_count,
                id: m.id,
                label,
                missing,
                anchor: m.anchor.clone(),
            });
            cursor = m.char_range.end;
        }
        let tail: String = chars[cursor..].iter().collect();
        new_line.push_str(&tail);
        rendered_lines.push(new_line);
    }

    let mut rendered_content = rendered_lines.join("\n");
    if content.ends_with('\n') {
        rendered_content.push('\n');
    }
    (rendered_content, rendered_mentions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn entry(id: u32, title: &str, content: &str, tags: &[&str]) -> Entry {
        Entry::new(
            id,
            Utc.with_ymd_and_hms(2026, 5, 15, 12, 0, 0).unwrap(),
            title.to_string(),
            content.to_string(),
            tags.iter().map(|s| s.to_string()).collect(),
            None,
        )
    }

    fn dd_mm_yyyy() -> DateFormat {
        DateFormat::new("DD-MM-YYYY")
    }

    #[test]
    fn render_label_with_title() {
        let e = entry(1, "My Title", "body", &[]);
        assert_eq!(
            render_mention_label(&e, &dd_mm_yyyy()),
            "My Title (15-05-2026)"
        );
    }

    #[test]
    fn render_label_blank_title_falls_back_to_date_only() {
        let e = entry(1, "", "body", &[]);
        assert_eq!(render_mention_label(&e, &dd_mm_yyyy()), "15-05-2026");
    }

    #[test]
    fn render_label_whitespace_title_treated_as_blank() {
        let e = entry(1, "   ", "body", &[]);
        assert_eq!(render_mention_label(&e, &dd_mm_yyyy()), "15-05-2026");
    }

    #[test]
    fn substitute_no_mentions_returns_content_unchanged() {
        let content = "no tokens here\nsecond line";
        let (out, mentions) = substitute_mentions(content, &[], &dd_mm_yyyy());
        assert_eq!(out, content);
        assert!(mentions.is_empty());
    }

    #[test]
    fn substitute_present_entry_inserts_label() {
        let entries = vec![entry(159, "My Day", "body", &[])];
        let content = "see @id:159 today";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "see My Day (15-05-2026) today");
        assert_eq!(mentions.len(), 1);
        assert!(!mentions[0].missing);
        assert_eq!(mentions[0].label, "My Day (15-05-2026)");
        assert_eq!(mentions[0].source_char_range, 4..11);
        assert_eq!(mentions[0].label_char_range, 4..23);
    }

    #[test]
    fn substitute_missing_entry_keeps_raw_token_with_missing_flag() {
        let content = "see @id:99999 here";
        let (out, mentions) = substitute_mentions(content, &[], &dd_mm_yyyy());
        assert_eq!(out, "see @id:99999 here");
        assert_eq!(mentions.len(), 1);
        assert!(mentions[0].missing);
        assert_eq!(mentions[0].label, "@id:99999");
    }

    #[test]
    fn substitute_blank_title_uses_date_only() {
        let entries = vec![entry(7, "", "body", &[])];
        let content = "ref @id:7";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "ref 15-05-2026");
        assert_eq!(mentions[0].label, "15-05-2026");
    }

    #[test]
    fn substitute_handles_multiple_tokens_on_same_line() {
        let entries = vec![entry(1, "A", "x", &[]), entry(2, "B", "y", &[])];
        let content = "@id:1 then @id:2";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "A (15-05-2026) then B (15-05-2026)");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0].id, 1);
        assert_eq!(mentions[1].id, 2);
        let chars: Vec<char> = out.chars().collect();
        let label0: String = chars[mentions[0].label_char_range.clone()].iter().collect();
        let label1: String = chars[mentions[1].label_char_range.clone()].iter().collect();
        assert_eq!(label0, "A (15-05-2026)");
        assert_eq!(label1, "B (15-05-2026)");
    }

    #[test]
    fn substitute_skips_tokens_inside_code_fence() {
        let entries = vec![entry(1, "Title", "body", &[])];
        let content = "ref @id:1\n```\n@id:1 inside\n```\nend";
        let (out, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(out, "ref Title (15-05-2026)\n```\n@id:1 inside\n```\nend");
        assert_eq!(mentions.len(), 1);
    }

    #[test]
    fn substitute_treats_soft_deleted_entry_as_missing() {
        let mut deleted = entry(5, "Gone", "body", &[]);
        deleted.deleted_at = Some(Utc.with_ymd_and_hms(2026, 5, 14, 0, 0, 0).unwrap());
        let entries = vec![deleted];
        let content = "see @id:5";
        let (_, mentions) = substitute_mentions(content, &entries, &dd_mm_yyyy());
        assert_eq!(mentions.len(), 1);
        assert!(mentions[0].missing);
    }
}
