use anyhow::Ok;
use chrono::{Datelike, Local, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tui_textarea::{CursorMove, TextArea};

use crate::{
    app::{App, keymap::Input, templates::Template},
    settings::{DateFormat, Settings},
};

use backend::{DataProvider, Entry};

use self::tags::{TagsPopup, TagsPopupReturn};

use super::{Styles, ui_functions::centered_rect_exact_height};

mod category_autocomplete;
mod fuzzy_suggestions;
mod tags;
mod tags_autocomplete;

const FOOTER_TEXT: &str = "Enter or <Ctrl-m>: confirm | Esc or <Ctrl-c>: Cancel | Tab: Change focused control | <Ctrl-Space> or <Ctrl-t>: Open tags";
const FOOTER_MARGIN: u16 = 15;

pub struct EntryPopup<'a> {
    title_txt: TextArea<'a>,
    date_txt: TextArea<'a>,
    tags_txt: TextArea<'a>,
    priority_txt: TextArea<'a>,
    category_txt: TextArea<'a>,
    is_edit_entry: bool,
    active_txt: ActiveText,
    title_err_msg: String,
    date_err_msg: String,
    tags_err_msg: String,
    priority_err_msg: String,
    category_err_msg: String,
    tags_popup: Option<TagsPopup>,
    tag_suggestions: Option<fuzzy_suggestions::SuggestionState>,
    category_suggestions: Option<fuzzy_suggestions::SuggestionState>,
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
    Category,
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntryPopupInputReturn {
    KeepPopup,
    Cancel,
    AddEntry(u32),
    UpdateCurrentEntry,
}

struct PopupFields<'a> {
    title_txt: TextArea<'a>,
    date_txt: TextArea<'a>,
    tags_txt: TextArea<'a>,
    priority_txt: TextArea<'a>,
    category_txt: TextArea<'a>,
    is_edit_entry: bool,
    template_content: Option<String>,
}

impl<'a> EntryPopup<'a> {
    fn build(fields: PopupFields<'a>, settings: &Settings) -> Self {
        Self {
            title_txt: fields.title_txt,
            date_txt: fields.date_txt,
            tags_txt: fields.tags_txt,
            priority_txt: fields.priority_txt,
            category_txt: fields.category_txt,
            is_edit_entry: fields.is_edit_entry,
            active_txt: ActiveText::Title,
            title_err_msg: String::default(),
            date_err_msg: String::default(),
            tags_err_msg: String::default(),
            priority_err_msg: String::default(),
            category_err_msg: String::default(),
            tags_popup: None,
            tag_suggestions: None,
            category_suggestions: None,
            template_content: fields.template_content,
            date_format: settings.date_format.clone(),
        }
    }

    pub fn new_entry(settings: &Settings, default_category: &str) -> Self {
        let date = Local::now();
        let priority_txt = match settings.default_journal_priority {
            Some(priority) => TextArea::new(vec![priority.to_string()]),
            None => TextArea::default(),
        };
        Self::build(
            PopupFields {
                title_txt: TextArea::default(),
                date_txt: TextArea::new(vec![settings.date_format.display(&date)]),
                tags_txt: TextArea::default(),
                priority_txt,
                category_txt: TextArea::new(vec![default_category.to_owned()]),
                is_edit_entry: false,
                template_content: None,
            },
            settings,
        )
    }

    /// Seeds a new-entry popup from a template. Title, tags, and priority
    /// come from the template's frontmatter (any can be overridden by the
    /// user before confirming). Date defaults to today. Content is the
    /// template's body.
    pub fn from_template(template: &Template, settings: &Settings, default_category: &str) -> Self {
        let date = Local::now();
        let priority_value = template.priority.or(settings.default_journal_priority);
        let priority_txt = match priority_value {
            Some(priority) => TextArea::new(vec![priority.to_string()]),
            None => TextArea::default(),
        };
        let mut popup = Self::build(
            PopupFields {
                title_txt: TextArea::new(vec![template.title.clone().unwrap_or_default()]),
                date_txt: TextArea::new(vec![settings.date_format.display(&date)]),
                tags_txt: TextArea::new(vec![tags_to_text(&template.tags)]),
                priority_txt,
                category_txt: TextArea::new(vec![default_category.to_owned()]),
                is_edit_entry: false,
                template_content: Some(template.content.clone()),
            },
            settings,
        );
        popup.validate_all();
        popup
    }

    pub fn from_entry(entry: &Entry, settings: &Settings) -> Self {
        let mut title_txt = TextArea::new(vec![entry.title.to_owned()]);
        title_txt.move_cursor(CursorMove::End);

        let mut tags_txt = TextArea::new(vec![tags_to_text(&entry.tags)]);
        tags_txt.move_cursor(CursorMove::End);

        let prio = entry.priority.map(|pr| pr.to_string()).unwrap_or_default();
        let mut priority_txt = TextArea::new(vec![prio]);
        priority_txt.move_cursor(CursorMove::End);

        let mut category_txt = TextArea::new(vec![entry.category.clone()]);
        category_txt.move_cursor(CursorMove::End);

        let mut popup = Self::build(
            PopupFields {
                title_txt,
                date_txt: TextArea::new(vec![settings.date_format.display(&entry.date)]),
                tags_txt,
                priority_txt,
                category_txt,
                is_edit_entry: true,
                template_content: None,
            },
            settings,
        );
        popup.validate_all();
        popup
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        // Source-of-truth for the popup's vertical layout. Height is derived
        // from these so adding a new field below just means appending another
        // Constraint::Length(3) row and the popup auto-resizes.
        const FIELD_CONSTRAINTS: &[Constraint] = &[
            Constraint::Length(3), // Title
            Constraint::Length(3), // Date
            Constraint::Length(3), // Priority
            Constraint::Length(3), // Category
            Constraint::Length(3), // Tags
            Constraint::Min(2),    // Footer (Min(2) so it gets 2 rows when there's room, can wrap)
        ];

        let target_height: u16 = FIELD_CONSTRAINTS
            .iter()
            .map(|c| match c {
                Constraint::Length(n) => *n,
                Constraint::Min(n) => *n,
                _ => 0,
            })
            .sum::<u16>()
            + 4; // vertical_margin(2) at top + bottom

        let mut area = centered_rect_exact_height(70, target_height, area);

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
            .constraints(FIELD_CONSTRAINTS)
            .split(area);

        self.title_txt.set_cursor_line_style(Style::default());
        self.date_txt.set_cursor_line_style(Style::default());
        self.tags_txt.set_cursor_line_style(Style::default());
        self.priority_txt.set_cursor_line_style(Style::default());
        self.category_txt.set_cursor_line_style(Style::default());

        let field_styles = FieldStyles::new(styles);

        render_field(
            &mut self.title_txt,
            self.active_txt == ActiveText::Title,
            &self.title_err_msg,
            "Title",
            None,
            &field_styles,
        );
        render_field(
            &mut self.date_txt,
            self.active_txt == ActiveText::Date,
            &self.date_err_msg,
            "Date",
            None,
            &field_styles,
        );
        render_field(
            &mut self.tags_txt,
            self.active_txt == ActiveText::Tags,
            &self.tags_err_msg,
            "Tags",
            Some("Tags - comma-separated | <Ctrl-T>: browse existing"),
            &field_styles,
        );
        render_field(
            &mut self.priority_txt,
            self.active_txt == ActiveText::Priority,
            &self.priority_err_msg,
            "Priority",
            None,
            &field_styles,
        );
        render_field(
            &mut self.category_txt,
            self.active_txt == ActiveText::Category,
            &self.category_err_msg,
            "Category",
            None,
            &field_styles,
        );

        frame.render_widget(&self.title_txt, chunks[0]);
        frame.render_widget(&self.date_txt, chunks[1]);
        frame.render_widget(&self.priority_txt, chunks[2]);
        frame.render_widget(&self.category_txt, chunks[3]);
        frame.render_widget(&self.tags_txt, chunks[4]);

        let footer = Paragraph::new(FOOTER_TEXT)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::NONE)
                    .style(Style::default()),
            );

        frame.render_widget(footer, chunks[5]);

        if matches!(self.active_txt, ActiveText::Tags)
            && let Some(state) = self.tag_suggestions.as_ref()
        {
            fuzzy_suggestions::render_overlay(
                frame,
                chunks[4],
                state,
                "Tags — Tab/Enter insert, Esc dismiss",
            );
        }

        if matches!(self.active_txt, ActiveText::Category)
            && let Some(state) = self.category_suggestions.as_ref()
        {
            fuzzy_suggestions::render_overlay(
                frame,
                chunks[3],
                state,
                "Category — Tab/Enter set, Esc dismiss",
            );
        }

        if let Some(tags_popup) = self.tags_popup.as_mut() {
            tags_popup.render_widget(frame, area, styles)
        }
    }

    pub fn is_input_valid(&self) -> bool {
        self.title_err_msg.is_empty()
            && self.date_err_msg.is_empty()
            && self.tags_err_msg.is_empty()
            && self.priority_err_msg.is_empty()
            && self.category_err_msg.is_empty()
    }

    pub fn validate_all(&mut self) {
        self.validate_title();
        self.validate_date();
        self.validate_tags();
        self.validate_priority();
        self.validate_category();
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

    fn validate_category(&mut self) {
        let category = self
            .category_txt
            .lines()
            .first()
            .map(|l| l.trim())
            .unwrap_or_default();
        if category.is_empty() {
            self.category_err_msg = String::from("Category cannot be empty");
        } else if category.contains(',') {
            self.category_err_msg = String::from("Category cannot contain a comma");
        } else {
            self.category_err_msg.clear();
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

        // Ctrl-Backspace, Alt-Backspace (= Option-Backspace on macOS), or
        // Ctrl-W in the tags field deletes the whole tag at the cursor
        // instead of one character / one word. Fires regardless of whether
        // the autocomplete overlay is visible.
        let has_word_modifier = has_ctrl || input.modifiers.contains(KeyModifiers::ALT);
        if matches!(self.active_txt, ActiveText::Tags)
            && has_word_modifier
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

        // Same shape for the category overlay.
        if self.category_suggestions.is_some() && matches!(self.active_txt, ActiveText::Category) {
            match input.key_code {
                KeyCode::Down => {
                    if let Some(state) = self.category_suggestions.as_mut() {
                        state.move_down();
                    }
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Up => {
                    if let Some(state) = self.category_suggestions.as_mut() {
                        state.move_up();
                    }
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Tab | KeyCode::Enter => {
                    self.apply_selected_category();
                    return Ok(EntryPopupInputReturn::KeepPopup);
                }
                KeyCode::Esc => {
                    self.category_suggestions = None;
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
                    ActiveText::Priority => ActiveText::Category,
                    ActiveText::Category => ActiveText::Tags,
                    ActiveText::Tags => ActiveText::Title,
                };
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Up => {
                self.active_txt = match self.active_txt {
                    ActiveText::Title => ActiveText::Tags,
                    ActiveText::Date => ActiveText::Title,
                    ActiveText::Priority => ActiveText::Date,
                    ActiveText::Category => ActiveText::Priority,
                    ActiveText::Tags => ActiveText::Category,
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
                    ActiveText::Category => {
                        if self.category_txt.input(KeyEvent::from(input)) {
                            self.validate_category();
                        }
                    }
                }
                Ok(EntryPopupInputReturn::KeepPopup)
            }
        };

        self.recompute_tag_suggestions(app);
        self.recompute_category_suggestions(app);
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
        self.tag_suggestions = fuzzy_suggestions::SuggestionState::build(query, &tags, true);
    }

    fn recompute_category_suggestions<D: DataProvider>(&mut self, app: &App<D>) {
        if !matches!(self.active_txt, ActiveText::Category) {
            self.category_suggestions = None;
            return;
        }
        let line = self
            .category_txt
            .lines()
            .first()
            .cloned()
            .unwrap_or_default();
        let categories = crate::app::categories::ordered_categories(&app.entries);
        self.category_suggestions = category_autocomplete::build_state(&line, &categories);
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
        let Some(tag) = state.selected_value().map(str::to_owned) else {
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

    fn apply_selected_category(&mut self) {
        let Some(state) = &self.category_suggestions else {
            return;
        };
        let Some(value) = state.selected_value().map(str::to_owned) else {
            return;
        };
        let mut new_field = TextArea::new(vec![value.clone()]);
        new_field.move_cursor(CursorMove::End);
        self.category_txt = new_field;
        self.category_suggestions = None;
        self.validate_category();
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

        let category = self
            .category_txt
            .lines()
            .first()
            .map(|l| l.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| backend::DEFAULT_CATEGORY.to_owned());

        if self.is_edit_entry {
            app.update_current_entry_attributes(title, date, tags, priority, category)
                .await?;
            Ok(EntryPopupInputReturn::UpdateCurrentEntry)
        } else {
            let entry_id = match self.template_content.take() {
                Some(content) if !content.is_empty() => {
                    app.add_entry_with_content(title, date, tags, priority, category, content)
                        .await?
                }
                _ => app.add_entry(title, date, tags, priority, category).await?,
            };
            Ok(EntryPopupInputReturn::AddEntry(entry_id))
        }
    }
}

struct FieldStyles {
    active_block: Style,
    invalid_block: Style,
    reset_block: Style,
    active_cursor: Style,
    invalid_cursor: Style,
    deactivate_cursor: Style,
}

impl FieldStyles {
    fn new(styles: &Styles) -> Self {
        let g = &styles.general;
        Self {
            active_block: Style::from(g.input_block_active),
            invalid_block: Style::from(g.input_block_invalid),
            reset_block: Style::reset(),
            active_cursor: Style::from(g.input_cursor_active),
            invalid_cursor: Style::from(g.input_cursor_invalid),
            deactivate_cursor: Style::default().bg(Color::Reset),
        }
    }
}

fn render_field(
    txt: &mut TextArea,
    is_active: bool,
    err: &str,
    normal_title: &str,
    active_title: Option<&str>,
    styles: &FieldStyles,
) {
    if err.is_empty() {
        let (block_style, cursor_style) = if is_active {
            (styles.active_block, styles.active_cursor)
        } else {
            (styles.reset_block, styles.deactivate_cursor)
        };
        let title = if is_active {
            active_title.unwrap_or(normal_title)
        } else {
            normal_title
        };
        txt.set_style(block_style);
        txt.set_cursor_style(cursor_style);
        txt.set_block(
            Block::default()
                .borders(Borders::ALL)
                .style(block_style)
                .title(title.to_string()),
        );
    } else {
        let cursor_style = if is_active {
            styles.invalid_cursor
        } else {
            styles.deactivate_cursor
        };
        txt.set_style(styles.invalid_block);
        txt.set_cursor_style(cursor_style);
        txt.set_block(
            Block::default()
                .borders(Borders::ALL)
                .style(styles.invalid_block)
                .title(format!("{normal_title} : {err}")),
        );
    }
}

fn tags_to_text(tags: &[String]) -> String {
    tags.join(", ")
}

fn text_to_tags(text: &str) -> Vec<String> {
    text.split(',')
        .map(|tag| tag.trim().to_owned())
        .filter(|tag| !tag.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{tags_to_text, text_to_tags};

    #[test]
    fn text_to_tags_strips_trailing_comma_space() {
        assert_eq!(text_to_tags("post, "), vec!["post"]);
    }

    #[test]
    fn text_to_tags_strips_trailing_comma_only() {
        assert_eq!(text_to_tags("post,"), vec!["post"]);
    }

    #[test]
    fn text_to_tags_skips_double_commas() {
        assert_eq!(text_to_tags("a,,b"), vec!["a", "b"]);
    }

    #[test]
    fn text_to_tags_skips_leading_comma() {
        assert_eq!(text_to_tags(",a,b"), vec!["a", "b"]);
    }

    #[test]
    fn text_to_tags_handles_only_commas_and_whitespace() {
        assert!(text_to_tags(", , ,").is_empty());
    }

    #[test]
    fn text_to_tags_preserves_emoji_prefixed_tags() {
        assert_eq!(
            text_to_tags("📝 Post, 🧵 Thread, "),
            vec!["📝 Post", "🧵 Thread"]
        );
    }

    #[test]
    fn tags_to_text_round_trips_through_text_to_tags() {
        let original = vec!["alpha".to_owned(), "beta".to_owned()];
        let text = tags_to_text(&original);
        assert_eq!(text_to_tags(&text), original);
    }
}
