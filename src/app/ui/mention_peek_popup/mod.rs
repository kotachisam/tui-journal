use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
};

use backend::Entry;

use crate::{app::keymap::Input, settings::DateFormat};

use super::{Styles, ui_functions::centered_rect};

const FOOTER_TEXT: &str = "Esc, q or <Ctrl-c>: Close | j/k or Up/Down: Scroll | PgDn/PgUp: Page";
const FOOTER_MARGIN: u16 = 4;
const PAGE_STEP: u16 = 10;

pub enum MentionPeekReturn {
    Keep,
    Close,
}

pub struct MentionPeekPopup {
    entry_id: u32,
    scroll: u16,
}

impl MentionPeekPopup {
    pub fn new(entry_id: u32) -> Self {
        Self {
            entry_id,
            scroll: 0,
        }
    }

    pub fn render_widget(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        styles: &Styles,
        entries: &[Entry],
        date_format: &DateFormat,
    ) {
        let _ = styles;
        let area = centered_rect(70, 70, area);
        let entry = entries.iter().find(|e| e.id == self.entry_id);

        let title = match entry {
            Some(e) => {
                let label = if e.title.trim().is_empty() {
                    "(untitled)".to_owned()
                } else {
                    e.title.clone()
                };
                let date = date_format.display(&e.date);
                let suffix = if e.deleted_at.is_some() {
                    " (deleted)"
                } else {
                    ""
                };
                format!("#{} · {} · {}{}", e.id, date, label, suffix)
            }
            None => format!("#{} · (not found)", self.entry_id),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(title);

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

        let body = match entry {
            Some(e) if e.content.trim().is_empty() => "(empty)".to_owned(),
            Some(e) => e.content.clone(),
            None => format!(
                "Entry @id:{} not found.\nIt may have been hard-deleted or never existed.",
                self.entry_id
            ),
        };

        let paragraph = Paragraph::new(body)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .block(Block::default());
        frame.render_widget(paragraph, chunks[0]);

        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(footer, chunks[1]);
    }

    pub fn handle_input(&mut self, input: &Input) -> MentionPeekReturn {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);
        match input.key_code {
            KeyCode::Esc | KeyCode::Char('q') => MentionPeekReturn::Close,
            KeyCode::Char('c') if has_control => MentionPeekReturn::Close,
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                MentionPeekReturn::Keep
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                MentionPeekReturn::Keep
            }
            KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(PAGE_STEP);
                MentionPeekReturn::Keep
            }
            KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(PAGE_STEP);
                MentionPeekReturn::Keep
            }
            KeyCode::Home => {
                self.scroll = 0;
                MentionPeekReturn::Keep
            }
            _ => MentionPeekReturn::Keep,
        }
    }
}
