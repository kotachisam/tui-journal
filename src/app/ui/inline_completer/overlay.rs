use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState},
};

use super::state::InlineCompleterState;

const DEFAULT_OVERLAY_WIDTH: u16 = 80;

pub trait CompletionProvider {
    type Candidate;

    fn render_candidate<'a>(&self, candidate: &'a Self::Candidate) -> Line<'a>;
    fn title(&self, selected: Option<&Self::Candidate>) -> String;

    fn overlay_width(&self) -> u16 {
        DEFAULT_OVERLAY_WIDTH
    }
}

pub fn render_overlay<P>(
    frame: &mut Frame,
    anchor: Rect,
    state: &InlineCompleterState<P::Candidate>,
    provider: &P,
) where
    P: CompletionProvider,
{
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
    let overlay_width = provider.overlay_width().min(max_width);
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
        .map(|c| ListItem::new(provider.render_candidate(c)))
        .collect();

    let title = provider.title(state.selected());

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black));

    let mut list_state = ListState::default();
    list_state.select(Some(state.selected_idx));

    frame.render_widget(Clear, overlay_area);
    frame.render_stateful_widget(list, overlay_area, &mut list_state);
}
