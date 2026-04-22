use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{keymap::Input, templates::Template};

use super::{Styles, ui_functions::centered_rect};

const FOOTER_TEXT: &str =
    "Up/Down or j/k: Navigate | Enter or <Ctrl-m>: Select | Esc, q or <Ctrl-c>: Cancel";
const FOOTER_MARGINE: u16 = 4;

pub enum TemplatePopupReturn {
    Keep,
    Cancel,
    Apply(Template),
}

pub struct TemplatePopup {
    templates: Vec<Template>,
    state: ListState,
}

impl TemplatePopup {
    pub fn new(templates: Vec<Template>) -> Self {
        let mut state = ListState::default();
        if !templates.is_empty() {
            state.select(Some(0));
        }
        Self { templates, state }
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let area = centered_rect(60, 70, area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title("Pick a template");

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let footer_height = if area.width < FOOTER_TEXT.len() as u16 + FOOTER_MARGINE {
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

        self.render_list(frame, chunks[0], styles);

        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .style(Style::default()),
            );
        frame.render_widget(footer, chunks[1]);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let gstyles = &styles.general;

        let items: Vec<ListItem> = self
            .templates
            .iter()
            .map(|tpl| {
                let label = match tpl.title.as_deref() {
                    Some(title) if !title.is_empty() => format!("{} — {}", tpl.name, title),
                    _ => tpl.name.clone(),
                };
                ListItem::new(label)
            })
            .collect();

        let list = List::new(items)
            .highlight_style(gstyles.list_highlight_active)
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, area, &mut self.state);
    }

    pub fn handle_input(&mut self, input: &Input) -> TemplatePopupReturn {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);
        match input.key_code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cycle_next();
                TemplatePopupReturn::Keep
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.cycle_prev();
                TemplatePopupReturn::Keep
            }
            KeyCode::Esc | KeyCode::Char('q') => TemplatePopupReturn::Cancel,
            KeyCode::Char('c') if has_control => TemplatePopupReturn::Cancel,
            KeyCode::Enter => self.apply(),
            KeyCode::Char('m') if has_control => self.apply(),
            _ => TemplatePopupReturn::Keep,
        }
    }

    fn cycle_next(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        let last = self.templates.len() - 1;
        let next = self
            .state
            .selected()
            .map(|idx| if idx >= last { 0 } else { idx + 1 })
            .unwrap_or(0);
        self.state.select(Some(next));
    }

    fn cycle_prev(&mut self) {
        if self.templates.is_empty() {
            return;
        }
        let last = self.templates.len() - 1;
        let prev = self
            .state
            .selected()
            .map(|idx| idx.checked_sub(1).unwrap_or(last))
            .unwrap_or(last);
        self.state.select(Some(prev));
    }

    fn apply(&self) -> TemplatePopupReturn {
        self.state
            .selected()
            .and_then(|idx| self.templates.get(idx).cloned())
            .map(TemplatePopupReturn::Apply)
            .unwrap_or(TemplatePopupReturn::Cancel)
    }
}
