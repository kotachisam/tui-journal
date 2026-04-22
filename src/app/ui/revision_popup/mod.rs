use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use backend::EntryRevision;

use crate::app::keymap::Input;

use super::{Styles, ui_functions::centered_rect};

const FOOTER_TEXT: &str =
    "Up/Down or j/k: Navigate | Esc, q or <Ctrl-c>: Close";
const FOOTER_MARGIN: u16 = 4;

pub enum RevisionPopupReturn {
    Keep,
    Close,
}

pub struct RevisionPopup {
    revisions: Vec<EntryRevision>,
    state: ListState,
    entry_title_for_header: String,
}

impl RevisionPopup {
    pub fn new(revisions: Vec<EntryRevision>, entry_title: String) -> Self {
        let mut state = ListState::default();
        if !revisions.is_empty() {
            state.select(Some(0));
        }
        Self {
            revisions,
            state,
            entry_title_for_header: entry_title,
        }
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let area = centered_rect(80, 80, area);

        let label = if self.entry_title_for_header.trim().is_empty() {
            "(untitled entry)".to_owned()
        } else {
            self.entry_title_for_header.clone()
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!("History — {label}"));

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let footer_height = if area.width < FOOTER_TEXT.len() as u16 + FOOTER_MARGIN {
            3
        } else {
            2
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(1)
            .vertical_margin(1)
            .constraints([Constraint::Min(3), Constraint::Length(footer_height)].as_ref())
            .split(area);

        if self.revisions.is_empty() {
            self.render_empty_state(frame, chunks[0]);
        } else {
            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)].as_ref())
                .split(chunks[0]);

            self.render_list(frame, panes[0], styles);
            self.render_preview(frame, panes[1]);
        }

        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(footer, chunks[1]);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let items: Vec<ListItem> = self
            .revisions
            .iter()
            .map(|rev| {
                let label = format!("{}  {}", rev.saved_at.format("%Y-%m-%d %H:%M"), truncate(&rev.title, 24));
                ListItem::new(label)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::RIGHT))
            .highlight_style(styles.general.list_highlight_active)
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    fn render_preview(&self, frame: &mut Frame, area: Rect) {
        let selected = self
            .state
            .selected()
            .and_then(|idx| self.revisions.get(idx));

        let text = match selected {
            Some(rev) => {
                let mut out = String::new();
                out.push_str(&format!("Title: {}\n", rev.title));
                out.push_str(&format!("Date:  {}\n", rev.date.format("%Y-%m-%d")));
                if !rev.tags.is_empty() {
                    out.push_str(&format!("Tags:  {}\n", rev.tags.join(", ")));
                }
                if let Some(prio) = rev.priority {
                    out.push_str(&format!("Prio:  {prio}\n"));
                }
                out.push('\n');
                out.push_str(&rev.content);
                out
            }
            None => String::from("No revision selected."),
        };

        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(Block::default());
        frame.render_widget(paragraph, area);
    }

    fn render_empty_state(&self, frame: &mut Frame, area: Rect) {
        let msg = Paragraph::new("\nNo revisions yet. This entry hasn't been edited since creation.")
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });
        frame.render_widget(msg, area);
    }

    pub fn handle_input(&mut self, input: &Input) -> RevisionPopupReturn {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);
        match input.key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cycle_next();
                RevisionPopupReturn::Keep
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cycle_prev();
                RevisionPopupReturn::Keep
            }
            KeyCode::Esc | KeyCode::Char('q') => RevisionPopupReturn::Close,
            KeyCode::Char('c') if has_control => RevisionPopupReturn::Close,
            _ => RevisionPopupReturn::Keep,
        }
    }

    fn cycle_next(&mut self) {
        if self.revisions.is_empty() {
            return;
        }
        let last = self.revisions.len() - 1;
        let next = self
            .state
            .selected()
            .map(|idx| if idx >= last { 0 } else { idx + 1 })
            .unwrap_or(0);
        self.state.select(Some(next));
    }

    fn cycle_prev(&mut self) {
        if self.revisions.is_empty() {
            return;
        }
        let last = self.revisions.len() - 1;
        let prev = self
            .state
            .selected()
            .map(|idx| idx.checked_sub(1).unwrap_or(last))
            .unwrap_or(last);
        self.state.select(Some(prev));
    }
}

fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_owned()
    } else {
        let mut s: String = text.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    }
}
