use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use super::state::MentionState;

const OVERLAY_WIDTH: u16 = 80;

fn highlight_snippet<'a>(snippet: &'a str, match_indices: &[usize]) -> Line<'a> {
    if match_indices.is_empty() {
        return Line::from(snippet);
    }
    let match_set: std::collections::BTreeSet<usize> = match_indices.iter().copied().collect();
    let highlight = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let plain = Style::default();

    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut buf = String::new();
    let mut buf_is_match = false;
    let flush = |buf: &mut String, is_match: bool, spans: &mut Vec<Span<'a>>| {
        if buf.is_empty() {
            return;
        }
        let style = if is_match { highlight } else { plain };
        spans.push(Span::styled(std::mem::take(buf), style));
    };

    for (i, ch) in snippet.chars().enumerate() {
        let is_match = match_set.contains(&i);
        if !buf.is_empty() && is_match != buf_is_match {
            flush(&mut buf, buf_is_match, &mut spans);
        }
        buf.push(ch);
        buf_is_match = is_match;
    }
    flush(&mut buf, buf_is_match, &mut spans);
    Line::from(spans)
}

pub fn render_overlay(frame: &mut Frame, anchor: Rect, state: &MentionState) {
    if state.candidates.is_empty() {
        return;
    }

    let frame_area = frame.area();
    let desired_height = (state.candidates.len() as u16) + 2;
    let below_y = anchor.y + anchor.height;
    let space_below = frame_area.height.saturating_sub(below_y);

    let (overlay_y, overlay_height) = if space_below >= desired_height {
        (below_y, desired_height)
    } else if anchor.y >= desired_height {
        (anchor.y - desired_height, desired_height)
    } else {
        (below_y, space_below.max(3).min(desired_height))
    };

    let max_width = frame_area.width.saturating_sub(2).max(20);
    let overlay_width = OVERLAY_WIDTH.min(max_width);
    let overlay_x = if anchor.x + overlay_width > frame_area.width {
        frame_area.width.saturating_sub(overlay_width)
    } else {
        anchor.x
    };
    let overlay_area = Rect {
        x: overlay_x,
        y: overlay_y,
        width: overlay_width,
        height: overlay_height,
    };

    let items: Vec<ListItem> = state
        .candidates
        .iter()
        .map(|c| ListItem::new(highlight_snippet(&c.snippet, &c.match_indices)))
        .collect();

    let date_display = state
        .selected()
        .map(|c| c.date_display.as_str())
        .unwrap_or("");
    let title = if date_display.is_empty() {
        "Mention — Tab/Enter insert, Esc dismiss".to_owned()
    } else {
        format!("{date_display} — Tab/Enter insert, Esc dismiss")
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_idx));

    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(list, overlay_area, &mut list_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_snippet_with_no_indices_returns_single_unstyled_span() {
        let line = highlight_snippet("plain text", &[]);
        assert_eq!(line.spans.len(), 1);
        assert_eq!(line.spans[0].content, "plain text");
    }

    #[test]
    fn highlight_snippet_alternates_styled_and_plain_runs() {
        let line = highlight_snippet("naval ravikant", &[0, 1, 2, 3, 4]);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].content, "naval");
        assert_eq!(line.spans[0].style.fg, Some(Color::Yellow));
        assert_eq!(line.spans[1].content, " ravikant");
        assert_eq!(line.spans[1].style.fg, None);
    }
}
