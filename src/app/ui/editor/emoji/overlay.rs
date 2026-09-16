use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::ui::inline_completer::{self, CompletionProvider};

use super::candidates::EmojiCandidate;
use super::state::EmojiState;

const OVERLAY_WIDTH: u16 = 46;

struct EmojiProvider;

impl CompletionProvider for EmojiProvider {
    type Candidate = EmojiCandidate;

    fn render_candidate<'a>(&self, candidate: &'a Self::Candidate) -> Line<'a> {
        let mut spans = vec![Span::raw(candidate.emoji), Span::raw("  ")];
        spans.extend(highlight_shortcode(candidate.shortcode, &candidate.match_indices).spans);
        Line::from(spans)
    }

    fn title(&self, selected: Option<&Self::Candidate>) -> String {
        match selected {
            Some(c) => format!("{} — Tab/Enter insert, Esc dismiss", c.name),
            None => "Emoji — Tab/Enter insert, Esc dismiss".to_owned(),
        }
    }

    fn overlay_width(&self) -> u16 {
        OVERLAY_WIDTH
    }
}

fn highlight_shortcode<'a>(shortcode: &'a str, match_indices: &[usize]) -> Line<'a> {
    if match_indices.is_empty() {
        return Line::from(format!(":{shortcode}:"));
    }
    let match_set: std::collections::BTreeSet<usize> = match_indices.iter().copied().collect();
    let highlight = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let plain = Style::default();

    let mut spans: Vec<Span<'a>> = vec![Span::styled(":", plain)];
    let mut buf = String::new();
    let mut buf_is_match = false;
    for (i, ch) in shortcode.chars().enumerate() {
        let is_match = match_set.contains(&i);
        if !buf.is_empty() && is_match != buf_is_match {
            let style = if buf_is_match { highlight } else { plain };
            spans.push(Span::styled(std::mem::take(&mut buf), style));
        }
        buf.push(ch);
        buf_is_match = is_match;
    }
    if !buf.is_empty() {
        let style = if buf_is_match { highlight } else { plain };
        spans.push(Span::styled(buf, style));
    }
    spans.push(Span::styled(":", plain));
    Line::from(spans)
}

pub fn render_overlay(frame: &mut Frame, anchor: Rect, state: &EmojiState) {
    inline_completer::render_overlay(frame, anchor, state, &EmojiProvider);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmatched_shortcode_renders_wrapped_in_colons() {
        let line = highlight_shortcode("tada", &[]);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, ":tada:");
    }

    #[test]
    fn matched_prefix_is_highlighted() {
        let line = highlight_shortcode("tada", &[0, 1]);
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, ":tada:");
        assert!(
            line.spans
                .iter()
                .any(|s| s.content == "ta" && s.style.fg == Some(Color::Yellow))
        );
    }

    #[test]
    fn non_contiguous_matches_split_into_runs() {
        let line = highlight_shortcode("rocket", &[0, 3]);
        let rendered: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, ":rocket:");
        let highlighted: Vec<&str> = line
            .spans
            .iter()
            .filter(|s| s.style.fg == Some(Color::Yellow))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(highlighted, vec!["r", "k"]);
    }

    #[test]
    fn candidate_line_leads_with_the_emoji() {
        let candidate = EmojiCandidate {
            emoji: "🎉",
            shortcode: "tada",
            name: "party popper",
            match_indices: vec![],
        };
        let line = EmojiProvider.render_candidate(&candidate);
        assert_eq!(line.spans[0].content, "🎉");
    }

    #[test]
    fn title_uses_the_selected_candidate_name() {
        let candidate = EmojiCandidate {
            emoji: "🚀",
            shortcode: "rocket",
            name: "rocket",
            match_indices: vec![],
        };
        assert!(EmojiProvider.title(Some(&candidate)).starts_with("rocket"));
        assert!(EmojiProvider.title(None).starts_with("Emoji"));
    }
}
