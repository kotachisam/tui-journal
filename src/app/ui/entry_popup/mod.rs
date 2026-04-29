use anyhow::Ok;
use chrono::{Datelike, Local, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use tui_textarea::{CursorMove, TextArea};

use crate::{
    app::{App, keymap::Input, templates::Template},
    settings::{DateFormat, Settings},
};

use backend::{DataProvider, Entry};

use self::tags::{TagsPopup, TagsPopupReturn};

use super::{Styles, ui_functions::centered_rect_exact_height};

mod tags;
mod tags_autocomplete;

const FOOTER_TEXT: &str = "Enter or <Ctrl-m>: confirm | Esc or <Ctrl-c>: Cancel | Tab: Change focused control | <Ctrl-Space> or <Ctrl-t>: Open tags";
const FOOTER_MARGIN: u16 = 15;

pub struct EntryPopup<'a> {
    title_txt: TextArea<'a>,
    date_txt: TextArea<'a>,
    tags_txt: TextArea<'a>,
    priority_txt: TextArea<'a>,
    is_edit_entry: bool,
    active_txt: ActiveText,
    title_err_msg: String,
    date_err_msg: String,
    tags_err_msg: String,
    priority_err_msg: String,
    tags_popup: Option<TagsPopup>,
    tag_suggestions: Option<tags_autocomplete::SuggestionState>,
    /// When set, the confirm path uses this as the new entry's content
    /// instead of creating an empty one. Populated by `from_template`.
    template_content: Option<String>,
    date_format: DateFormat,
}

#[derive(Debug, PartialEq, Eq)]
enum ActiveText {
    Title,
    Date,
    Tags,
    Priority,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntryPopupInputReturn {
    KeepPopup,
    Cancel,
    AddEntry(u32),
    UpdateCurrentEntry,
}

impl EntryPopup<'_> {
    pub fn new_entry(settings: &Settings) -> Self {
        let title_txt = TextArea::default();

        let date = Local::now();

        let date_txt = TextArea::new(vec![settings.date_format.display(&date)]);

        let tags_txt = TextArea::default();

        let priority_txt = if let Some(priority) = settings.default_journal_priority {
            TextArea::new(vec![priority.to_string()])
        } else {
            TextArea::default()
        };

        Self {
            title_txt,
            date_txt,
            tags_txt,
            priority_txt,
            is_edit_entry: false,
            active_txt: ActiveText::Title,
            title_err_msg: String::default(),
            date_err_msg: String::default(),
            tags_err_msg: String::default(),
            priority_err_msg: String::default(),
            tags_popup: None,
            tag_suggestions: None,
            template_content: None,
            date_format: settings.date_format.clone(),
        }
    }

    /// Seeds a new-entry popup from a template. Title, tags, and priority
    /// come from the template's frontmatter (any can be overridden by the
    /// user before confirming). Date defaults to today. Content is the
    /// template's body.
    pub fn from_template(template: &Template, settings: &Settings) -> Self {
        let title_txt = TextArea::new(vec![template.title.clone().unwrap_or_default()]);

        let date = Local::now();
        let date_txt = TextArea::new(vec![settings.date_format.display(&date)]);

        let tags_txt = TextArea::new(vec![tags_to_text(&template.tags)]);

        let priority_value = template.priority.or(settings.default_journal_priority);
        let priority_txt = if let Some(priority) = priority_value {
            TextArea::new(vec![priority.to_string()])
        } else {
            TextArea::default()
        };

        let mut popup = Self {
            title_txt,
            date_txt,
            tags_txt,
            priority_txt,
            is_edit_entry: false,
            active_txt: ActiveText::Title,
            title_err_msg: String::default(),
            date_err_msg: String::default(),
            tags_err_msg: String::default(),
            priority_err_msg: String::default(),
            tags_popup: None,
            tag_suggestions: None,
            template_content: Some(template.content.clone()),
            date_format: settings.date_format.clone(),
        };
        popup.validate_all();
        popup
    }

    pub fn from_entry(entry: &Entry, settings: &Settings) -> Self {
        let mut title_txt = TextArea::new(vec![entry.title.to_owned()]);
        title_txt.move_cursor(CursorMove::End);

        let date_txt = TextArea::new(vec![settings.date_format.display(&entry.date)]);

        let tags = tags_to_text(&entry.tags);

        let mut tags_txt = TextArea::new(vec![tags]);
        tags_txt.move_cursor(CursorMove::End);

        let prio = entry.priority.map(|pr| pr.to_string()).unwrap_or_default();

        let mut priority_txt = TextArea::new(vec![prio]);
        priority_txt.move_cursor(CursorMove::End);

        let mut entry_popup = Self {
            title_txt,
            date_txt,
            tags_txt,
            priority_txt,
            is_edit_entry: true,
            active_txt: ActiveText::Title,
            title_err_msg: String::default(),
            date_err_msg: String::default(),
            tags_err_msg: String::default(),
            priority_err_msg: String::default(),
            tags_popup: None,
            tag_suggestions: None,
            template_content: None,
            date_format: settings.date_format.clone(),
        };

        entry_popup.validate_all();

        entry_popup
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let mut area = centered_rect_exact_height(70, 17, area);

        const FOOTER_LEN: u16 = FOOTER_TEXT.len() as u16 + FOOTER_MARGIN;

        if area.width < FOOTER_LEN {
            area.height += FOOTER_LEN / area.width;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .title(if self.is_edit_entry {
                "Edit journal"
            } else {
                "Create journal"
            });

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(4)
            .vertical_margin(2)
            .constraints(
                [
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(area);

        self.title_txt.set_cursor_line_style(Style::default());
        self.date_txt.set_cursor_line_style(Style::default());
        self.tags_txt.set_cursor_line_style(Style::default());
        self.priority_txt.set_cursor_line_style(Style::default());

        let gstyles = &styles.general;

        let active_block_style = Style::from(gstyles.input_block_active);
        let reset_style = Style::reset();
        let invalid_block_style = Style::from(gstyles.input_block_invalid);

        let active_cursor_style = Style::from(gstyles.input_cursor_active);
        let deactivate_cursor_style = Style::default().bg(Color::Reset);
        let invalid_cursor_style = Style::from(gstyles.input_cursor_invalid);

        if self.title_err_msg.is_empty() {
            let (block, cursor) = match self.active_txt {
                ActiveText::Title => (active_block_style, active_cursor_style),
                _ => (reset_style, deactivate_cursor_style),
            };
            self.title_txt.set_style(block);
            self.title_txt.set_cursor_style(cursor);
            self.title_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title("Title"),
            );
        } else {
            let cursor = if self.active_txt == ActiveText::Title {
                invalid_cursor_style
            } else {
                deactivate_cursor_style
            };

            self.title_txt.set_style(invalid_block_style);
            self.title_txt.set_cursor_style(cursor);
            self.title_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(invalid_block_style)
                    .title(format!("Title : {}", self.title_err_msg)),
            );
        }

        if self.date_err_msg.is_empty() {
            let (block, cursor) = match self.active_txt {
                ActiveText::Date => (active_block_style, active_cursor_style),
                _ => (reset_style, deactivate_cursor_style),
            };
            self.date_txt.set_style(block);
            self.date_txt.set_cursor_style(cursor);
            self.date_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title("Date"),
            );
        } else {
            let cursor = if self.active_txt == ActiveText::Date {
                invalid_cursor_style
            } else {
                deactivate_cursor_style
            };
            self.date_txt.set_style(invalid_block_style);
            self.date_txt.set_cursor_style(cursor);
            self.date_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(invalid_block_style)
                    .title(format!("Date : {}", self.date_err_msg)),
            );
        }

        if self.tags_err_msg.is_empty() {
            let (block, cursor, title) = match self.active_txt {
                ActiveText::Tags => (
                    active_block_style,
                    active_cursor_style,
                    "Tags - comma-separated | <Ctrl-T>: browse existing",
                ),
                _ => (reset_style, deactivate_cursor_style, "Tags"),
            };
            self.tags_txt.set_style(block);
            self.tags_txt.set_cursor_style(cursor);
            self.tags_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title(title),
            );
        } else {
            let cursor = if self.active_txt == ActiveText::Tags {
                invalid_cursor_style
            } else {
                deactivate_cursor_style
            };
            self.tags_txt.set_style(invalid_block_style);
            self.tags_txt.set_cursor_style(cursor);
            self.tags_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(invalid_block_style)
                    .title(format!("Tags : {}", self.date_err_msg)),
            );
        }

        if self.priority_err_msg.is_empty() {
            let (block, cursor) = match self.active_txt {
                ActiveText::Priority => (active_block_style, active_cursor_style),
                _ => (reset_style, deactivate_cursor_style),
            };
            self.priority_txt.set_style(block);
            self.priority_txt.set_cursor_style(cursor);
            self.priority_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title("Priority"),
            );
        } else {
            let cursor = if self.active_txt == ActiveText::Priority {
                invalid_cursor_style
            } else {
                deactivate_cursor_style
            };
            self.priority_txt.set_style(invalid_block_style);
            self.priority_txt.set_cursor_style(cursor);
            self.priority_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(invalid_block_style)
                    .title(format!("Priority : {}", self.priority_err_msg)),
            );
        }

        frame.render_widget(&self.title_txt, chunks[0]);
        frame.render_widget(&self.date_txt, chunks[1]);
        frame.render_widget(&self.priority_txt, chunks[2]);
        frame.render_widget(&self.tags_txt, chunks[3]);

        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default()),
            );

        frame.render_widget(footer, chunks[4]);

        if matches!(self.active_txt, ActiveText::Tags) && self.tag_suggestions.is_some() {
            self.render_tag_autocomplete(frame, chunks[3]);
        }

        if let Some(tags_popup) = self.tags_popup.as_mut() {
            tags_popup.render_widget(frame, area, styles)
        }
    }

    fn render_tag_autocomplete(&self, frame: &mut Frame, tags_area: Rect) {
        let Some(state) = self.tag_suggestions.as_ref() else {
            return;
        };
        let matches = state.matches();
        if matches.is_empty() {
            return;
        }

        let frame_area = frame.area();
        let desired_height = (matches.len() as u16) + 2;
        let below_y = tags_area.y + tags_area.height;
        let space_below = frame_area.height.saturating_sub(below_y);

        let (overlay_y, overlay_height) = if space_below >= desired_height {
            (below_y, desired_height)
        } else if tags_area.y >= desired_height {
            (tags_area.y - desired_height, desired_height)
        } else {
            // Tight fit — clip below.
            (below_y, space_below.max(3).min(desired_height))
        };

        let overlay_width = tags_area.width.min(60);
        let overlay_area = Rect {
            x: tags_area.x,
            y: overlay_y,
            width: overlay_width,
            height: overlay_height,
        };

        let items: Vec<ListItem> = matches
            .iter()
            .map(|(_, tag)| ListItem::new(tag.as_str()))
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Tags — Tab/Enter insert, Esc dismiss"),
            )
            .highlight_style(Style::default().bg(Color::LightBlue).fg(Color::Black));

        let mut list_state = ListState::default();
        list_state.select(Some(state.selected_index()));

        frame.render_widget(Clear, overlay_area);
        frame.render_stateful_widget(list, overlay_area, &mut list_state);
    }

    pub fn is_input_valid(&self) -> bool {
        self.title_err_msg.is_empty()
            && self.date_err_msg.is_empty()
            && self.tags_err_msg.is_empty()
            && self.priority_err_msg.is_empty()
    }

    pub fn validate_all(&mut self) {
        self.validate_title();
        self.validate_date();
        self.validate_tags();
        self.validate_priority();
    }

    fn validate_title(&mut self) {
        self.title_err_msg.clear();
    }

    fn validate_date(&mut self) {
        if let Err(err) = self.date_format.parse(self.date_txt.lines()[0].as_str()) {
            self.date_err_msg = err.to_string();
        } else {
            self.date_err_msg.clear();
        }
    }

    fn validate_tags(&mut self) {
        let tags = text_to_tags(
            self.tags_txt
                .lines()
                .first()
                .expect("Tags TextBox have one line"),
        );
        if tags.iter().any(|tag| tag.contains(',')) {
            self.tags_err_msg = "Tags are invalid".into();
        } else {
            self.tags_err_msg.clear();
        }
    }

    fn validate_priority(&mut self) {
        let prio_text = self.priority_txt.lines().first().unwrap();
        if !prio_text.is_empty() && prio_text.parse::<u32>().is_err() {
            self.priority_err_msg = String::from("Priority must be a positive number");
        } else {
            self.priority_err_msg.clear();
        }
    }

    pub async fn handle_input<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> anyhow::Result<EntryPopupInputReturn> {
        if self.tags_popup.is_some() {
            self.handle_tags_popup_input(input);

            return Ok(EntryPopupInputReturn::KeepPopup);
        }

        let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);

        // Ctrl-Backspace (or Ctrl-W as a terminal-compat fallback) in the tags
        // field deletes the whole tag at the cursor instead of one character.
        // Fires regardless of whether the autocomplete overlay is visible.
        if matches!(self.active_txt, ActiveText::Tags)
            && has_ctrl
            && matches!(
                input.key_code,
                KeyCode::Backspace | KeyCode::Char('w') | KeyCode::Char('W')
            )
        {
            self.delete_tag_at_cursor();
            self.recompute_tag_suggestions(app);
            return Ok(EntryPopupInputReturn::KeepPopup);
        }

        // Tag autocomplete overlay intercepts Up/Down/Tab/Enter/Esc when visible
        // and the user is in the tags field. Other states fall through to the
        // existing field-cycle / confirm / cancel bindings.
        if self.tag_suggestions.is_some() && matches!(self.active_txt, ActiveText::Tags) {
            match input.key_code {
                KeyCode::Down => {
                    if let Some(state) = self.tag_suggestions.as_mut() {
                        state.move_down();
                    }
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Up => {
                    if let Some(state) = self.tag_suggestions.as_mut() {
                        state.move_up();
                    }
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.apply_selected_tag();
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Esc => {
                    self.tag_suggestions = None;
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                _ => {}
            }
        }

        let result: anyhow::Result<EntryPopupInputReturn> = match input.key_code {
            KeyCode::Esc => Ok(EntryPopupInputReturn::Cancel),
            KeyCode::Char('c') if has_ctrl => Ok(EntryPopupInputReturn::Cancel),
            KeyCode::Enter => self.handle_confirm(app).await,
            KeyCode::Tab | KeyCode::Down => {
                self.active_txt = match self.active_txt {
                    ActiveText::Title => ActiveText::Date,
                    ActiveText::Date => ActiveText::Priority,
                    ActiveText::Priority => ActiveText::Tags,
                    ActiveText::Tags => ActiveText::Title,
                };
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Up => {
                self.active_txt = match self.active_txt {
                    ActiveText::Title => ActiveText::Tags,
                    ActiveText::Date => ActiveText::Title,
                    ActiveText::Priority => ActiveText::Date,
                    ActiveText::Tags => ActiveText::Priority,
                };
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Char(' ') | KeyCode::Char('t') if has_ctrl => {
                debug_assert!(self.tags_popup.is_none());

                let tags = app.get_all_tags();
                let tags_text = self
                    .tags_txt
                    .lines()
                    .first()
                    .expect("Tags text box has one line");

                self.tags_popup = Some(TagsPopup::new(tags_text, tags));

                Ok(EntryPopupInputReturn::KeepPopup)
            }
            _ => {
                match self.active_txt {
                    ActiveText::Title => {
                        if self.title_txt.input(KeyEvent::from(input)) {
                            self.validate_title();
                        }
                    }
                    ActiveText::Date => {
                        if self.date_txt.input(KeyEvent::from(input)) {
                            self.validate_date();
                        }
                    }
                    ActiveText::Tags => {
                        if self.tags_txt.input(KeyEvent::from(input)) {
                            self.validate_tags();
                        }
                    }
                    ActiveText::Priority => {
                        if self.priority_txt.input(KeyEvent::from(input)) {
                            self.validate_priority();
                        }
                    }
                }
                Ok(EntryPopupInputReturn::KeepPopup)
            }
        };

        self.recompute_tag_suggestions(app);
        result
    }

    fn recompute_tag_suggestions<D: DataProvider>(&mut self, app: &App<D>) {
        if !matches!(self.active_txt, ActiveText::Tags) {
            self.tag_suggestions = None;
            return;
        }
        let line = self.tags_txt.lines().first().cloned().unwrap_or_default();
        let (_, col) = self.tags_txt.cursor();
        let query = tags_autocomplete::extract_active_query(&line, col);
        let tags = app.get_all_tags();
        self.tag_suggestions = tags_autocomplete::SuggestionState::build(query, &tags);
    }

    fn delete_tag_at_cursor(&mut self) {
        let line = self.tags_txt.lines().first().cloned().unwrap_or_default();
        let (_, cursor_char) = self.tags_txt.cursor();
        let Some((new_line, new_cursor_char)) =
            tags_autocomplete::delete_tag_at_cursor(&line, cursor_char)
        else {
            return;
        };
        let mut new_tags = TextArea::new(vec![new_line]);
        new_tags.move_cursor(CursorMove::Jump(0, new_cursor_char as u16));
        self.tags_txt = new_tags;
        self.tag_suggestions = None;
        self.validate_tags();
    }

    fn apply_selected_tag(&mut self) {
        let Some(state) = &self.tag_suggestions else {
            return;
        };
        let Some(tag) = state.selected_tag().map(str::to_owned) else {
            return;
        };
        let line = self.tags_txt.lines().first().cloned().unwrap_or_default();
        let (_, cursor_char) = self.tags_txt.cursor();

        let cursor_byte = tags_autocomplete::char_index_to_byte_index(&line, cursor_char);
        let start_byte = tags_autocomplete::active_query_start(&line, cursor_char);
        let suffix = ", ";

        let mut new_line = String::with_capacity(line.len() + tag.len() + suffix.len());
        new_line.push_str(&line[..start_byte]);
        new_line.push_str(&tag);
        new_line.push_str(suffix);
        new_line.push_str(&line[cursor_byte..]);

        // Cursor target is in CHARACTERS (CursorMove::Jump is char-based).
        let chars_before_splice = line[..start_byte].chars().count();
        let inserted_char_count = tag.chars().count() + suffix.chars().count();
        let new_cursor_char = chars_before_splice + inserted_char_count;

        let mut new_tags = TextArea::new(vec![new_line]);
        new_tags.move_cursor(CursorMove::Jump(0, new_cursor_char as u16));
        self.tags_txt = new_tags;
        self.tag_suggestions = None;
        self.validate_tags();
    }

    pub fn handle_tags_popup_input(&mut self, input: &Input) {
        let tags_popup = self
            .tags_popup
            .as_mut()
            .expect("Tags popup must be some at this point");

        match tags_popup.handle_input(input) {
            TagsPopupReturn::Keep => {}
            TagsPopupReturn::Cancel => self.tags_popup = None,
            TagsPopupReturn::Apply(tags_text) => {
                self.tags_txt = TextArea::new(vec![tags_text]);
                self.tags_txt.move_cursor(CursorMove::End);
                self.active_txt = ActiveText::Tags;
                self.tags_popup = None;
            }
        }
    }

    async fn handle_confirm<D: DataProvider>(
        &mut self,
        app: &mut App<D>,
    ) -> anyhow::Result<EntryPopupInputReturn> {
        // Validation
        self.validate_all();
        if !self.is_input_valid() {
            return Ok(EntryPopupInputReturn::KeepPopup);
        }

        let title = self.title_txt.lines()[0].to_owned();
        let date = self
            .date_format
            .parse(self.date_txt.lines()[0].as_str())
            .expect("Date must be valid here");

        let date = Utc
            .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
            .unwrap();

        let tags = text_to_tags(
            self.tags_txt
                .lines()
                .first()
                .expect("Tags TextBox have one line"),
        );

        let priority = match self.priority_txt.lines().first().unwrap() {
            num if num.is_empty() => None,
            num => Some(num.parse().expect("Priority must be validated before")),
        };

        if self.is_edit_entry {
            app.update_current_entry_attributes(title, date, tags, priority)
                .await?;
            Ok(EntryPopupInputReturn::UpdateCurrentEntry)
        } else {
            let entry_id = match self.template_content.take() {
                Some(content) if !content.is_empty() => {
                    app.add_entry_with_content(title, date, tags, priority, content)
                        .await?
                }
                _ => app.add_entry(title, date, tags, priority).await?,
            };
            Ok(EntryPopupInputReturn::AddEntry(entry_id))
        }
    }
}

fn tags_to_text(tags: &[String]) -> String {
    tags.join(", ")
}

fn text_to_tags(text: &str) -> Vec<String> {
    text.split_terminator(',')
        .map(|tag| String::from(tag.trim()))
        .collect()
}
