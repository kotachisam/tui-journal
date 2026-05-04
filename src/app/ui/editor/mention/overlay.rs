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
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use super::super::candidates::MentionCandidate;

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

    fn make_candidates(n: usize) -> Vec<MentionCandidate> {
        (0..n)
            .map(|i| MentionCandidate {
                id: (i + 1) as u32,
                snippet: format!("snippet-{i}"),
                match_indices: Vec::new(),
                date_display: format!("2026-05-{:02}", i + 1),
            })
            .collect()
    }

    fn render_to_buffer(width: u16, height: u16, anchor: Rect, state: &MentionState) -> Buffer {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_overlay(f, anchor, state);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn find_top_left_corner(buffer: &Buffer) -> Option<(u16, u16)> {
        let area = buffer.area();
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                if let Some(cell) = buffer.cell((x, y))
                    && cell.symbol() == "┌"
                {
                    return Some((x, y));
                }
            }
        }
        None
    }

    fn row_text(buffer: &Buffer, y: u16) -> String {
        let area = buffer.area();
        let mut s = String::new();
        for x in area.x..area.x + area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                s.push_str(cell.symbol());
            }
        }
        s
    }

    fn state_with(n: usize, selected_idx: usize) -> MentionState {
        let mut state = MentionState::new(0, 0);
        state.candidates = make_candidates(n);
        state.selected_idx = selected_idx;
        state
    }

    #[test]
    fn does_not_render_when_no_candidates() {
        let state = MentionState::new(0, 0);
        let anchor = Rect { x: 5, y: 5, width: 10, height: 1 };
        let buffer = render_to_buffer(80, 24, anchor, &state);
        assert!(
            find_top_left_corner(&buffer).is_none(),
            "no border should be rendered when candidates are empty"
        );
    }

    #[test]
    fn renders_below_anchor_when_space_available() {
        let state = state_with(3, 0);
        let anchor = Rect { x: 4, y: 2, width: 20, height: 1 };
        let buffer = render_to_buffer(120, 24, anchor, &state);
        let (x, y) = find_top_left_corner(&buffer).expect("expected overlay border");
        assert_eq!(y, anchor.y + anchor.height, "overlay should sit one row below anchor");
        assert_eq!(x, anchor.x, "overlay should align to anchor x when it fits");
    }

    #[test]
    fn flips_above_anchor_when_no_space_below() {
        let state = state_with(4, 0);
        let frame_h = 10u16;
        let anchor = Rect { x: 0, y: frame_h - 1, width: 10, height: 1 };
        let buffer = render_to_buffer(80, frame_h, anchor, &state);
        let (_, y) = find_top_left_corner(&buffer).expect("expected overlay border");
        let desired_height = state.candidates.len() as u16 + 2;
        assert_eq!(
            y,
            anchor.y - desired_height,
            "overlay should flip above the anchor when no room below"
        );
    }

    #[test]
    fn clamps_overlay_into_visible_frame_when_neither_fits_fully() {
        let state = state_with(20, 0);
        let frame_h = 8u16;
        let anchor = Rect { x: 0, y: 4, width: 10, height: 1 };
        let buffer = render_to_buffer(80, frame_h, anchor, &state);
        let (_, y) = find_top_left_corner(&buffer).expect("expected overlay border");
        assert!(
            y < frame_h,
            "overlay top must be inside visible frame (y={y}, frame_h={frame_h})"
        );
    }

    #[test]
    fn title_includes_date_display_of_selected_candidate() {
        let state = state_with(3, 1);
        let anchor = Rect { x: 0, y: 1, width: 10, height: 1 };
        let buffer = render_to_buffer(80, 24, anchor, &state);
        let (_, top_y) = find_top_left_corner(&buffer).expect("expected overlay border");
        let top_row = row_text(&buffer, top_y);
        assert!(
            top_row.contains("2026-05-02"),
            "top border row should contain the selected candidate's date_display, got: {top_row:?}"
        );
    }

    #[test]
    fn selected_row_has_distinct_background_styling() {
        let state = state_with(3, 1);
        let anchor = Rect { x: 0, y: 1, width: 10, height: 1 };
        let buffer = render_to_buffer(80, 24, anchor, &state);
        let (overlay_x, overlay_y) = find_top_left_corner(&buffer).expect("overlay rendered");
        let inside_x = overlay_x + 2;
        let selected_row_y = overlay_y + 1 + state.selected_idx as u16;
        let unselected_row_y = overlay_y + 1;

        let selected_bg = buffer
            .cell((inside_x, selected_row_y))
            .map(|c| c.bg)
            .expect("selected cell present");
        let unselected_bg = buffer
            .cell((inside_x, unselected_row_y))
            .map(|c| c.bg)
            .expect("unselected cell present");

        assert_ne!(
            selected_bg, unselected_bg,
            "selected row should have distinct bg from unselected row"
        );
    }
}
