use chrono::{Datelike, Local, NaiveDate, TimeZone, Utc};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tui_textarea::{CursorMove, TextArea};

use crate::{
    app::{App, keymap::Input, templates::Template},
    settings::{DateFormat, Settings},
};

use backend::{DataProvider, Entry};

use self::tags::{TagsPopup, TagsPopupReturn};

use super::{
    Styles,
    ui_functions::centered_rect_exact_height,
    widgets::{FieldStyles, render_field},
};

mod category_autocomplete;
mod fuzzy_suggestions;
mod tags;
mod tags_autocomplete;

const FOOTER_TEXT: &str = "Enter or <Ctrl-m>: confirm | <Ctrl-d>: confirm & next day | Esc or <Ctrl-c>: Cancel | Tab: Change focused control | <Ctrl-Space> or <Ctrl-t>: Open tags";
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
    priority_err_msg: String,
    category_err_msg: String,
    tags_popup: Option<TagsPopup>,
    tag_suggestions: Option<fuzzy_suggestions::SuggestionState>,
    category_suggestions: Option<fuzzy_suggestions::SuggestionState>,
    /// When set, the confirm path uses this as the new entry's content
    /// instead of creating an empty one. Populated by `from_template`.
    template_content: Option<String>,
    date_format: DateFormat,
    created_count: usize,
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
    AddEntry { focus_id: u32, count: usize },
    AddEntryContinue(u32),
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
            priority_err_msg: String::default(),
            category_err_msg: String::default(),
            tags_popup: None,
            tag_suggestions: None,
            category_suggestions: None,
            template_content: fields.template_content,
            date_format: settings.date_format.clone(),
            created_count: 0,
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
            Some(if self.is_edit_entry {
                "Date | <Ctrl-P/N> or <Opt-Up/Down>: adjust day"
            } else {
                "Date | <Ctrl-P/N>: adjust day | comma or start..end for several"
            }),
            &field_styles,
        );
        render_field(
            &mut self.tags_txt,
            self.active_txt == ActiveText::Tags,
            "",
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

        let footer_text = match self.created_count {
            0 => FOOTER_TEXT.to_owned(),
            count => format!("{count} created | {FOOTER_TEXT}"),
        };

        let footer = Paragraph::new(footer_text)
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
            && self.priority_err_msg.is_empty()
            && self.category_err_msg.is_empty()
    }

    pub fn validate_all(&mut self) {
        self.validate_title();
        self.validate_date();
        self.validate_priority();
        self.validate_category();
    }

    fn validate_title(&mut self) {
        self.title_err_msg.clear();
    }

    fn validate_date(&mut self) {
        match parse_date_batch(&self.date_format, self.date_txt.lines()[0].as_str()) {
            Err(err) => self.date_err_msg = err,
            Ok(dates) if self.is_edit_entry && dates.len() > 1 => {
                self.date_err_msg = String::from("An entry can only have one date");
            }
            Ok(_) => self.date_err_msg.clear(),
        }
    }

    fn step_date(&mut self, delta_days: i64) {
        let Some(stepped) = self
            .date_format
            .step(self.date_txt.lines()[0].as_str(), delta_days)
        else {
            return;
        };

        self.date_txt = TextArea::new(vec![stepped]);
        self.date_txt.move_cursor(CursorMove::End);
        self.validate_date();
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
        let has_alt = input.modifiers.contains(KeyModifiers::ALT);

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
            KeyCode::Enter => self.handle_confirm(app, false).await,
            KeyCode::Char('d') if has_ctrl && !self.is_edit_entry => {
                self.handle_confirm(app, true).await
            }
            KeyCode::Up if (has_ctrl || has_alt) && matches!(self.active_txt, ActiveText::Date) => {
                self.step_date(1);
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Down
                if (has_ctrl || has_alt) && matches!(self.active_txt, ActiveText::Date) =>
            {
                self.step_date(-1);
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Char('p') if has_ctrl && matches!(self.active_txt, ActiveText::Date) => {
                self.step_date(1);
                Ok(EntryPopupInputReturn::KeepPopup)
            }
            KeyCode::Char('n') if has_ctrl && matches!(self.active_txt, ActiveText::Date) => {
                self.step_date(-1);
                Ok(EntryPopupInputReturn::KeepPopup)
            }
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
            KeyCode::Up | KeyCode::BackTab => {
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
                        self.tags_txt.input(KeyEvent::from(input));
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
        keep_open: bool,
    ) -> anyhow::Result<EntryPopupInputReturn> {
        // Validation
        self.validate_all();
        if !self.is_input_valid() {
            return Ok(EntryPopupInputReturn::KeepPopup);
        }

        let title = self.title_txt.lines()[0].to_owned();
        let dates = parse_date_batch(&self.date_format, self.date_txt.lines()[0].as_str())
            .expect("Dates must be valid here");

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
            let date = to_utc_midnight(dates[0]);
            app.update_current_entry_attributes(title, date, tags, priority, category)
                .await?;
            return Ok(EntryPopupInputReturn::UpdateCurrentEntry);
        }

        let content = if keep_open {
            self.template_content.clone()
        } else {
            self.template_content.take()
        };

        let mut first_id = None;
        for day in &dates {
            let date = to_utc_midnight(*day);
            let entry_id = match content.as_ref() {
                Some(content) if !content.is_empty() => {
                    app.add_entry_with_content(
                        title.clone(),
                        date,
                        tags.clone(),
                        priority,
                        category.clone(),
                        content.clone(),
                    )
                    .await?
                }
                _ => {
                    app.add_entry(
                        title.clone(),
                        date,
                        tags.clone(),
                        priority,
                        category.clone(),
                    )
                    .await?
                }
            };
            first_id.get_or_insert(entry_id);
        }

        let focus_id = first_id.expect("A batch always has at least one date");

        if keep_open {
            self.advance_past(*dates.last().expect("A batch is never empty"), dates.len());
            Ok(EntryPopupInputReturn::AddEntryContinue(focus_id))
        } else {
            Ok(EntryPopupInputReturn::AddEntry {
                focus_id,
                count: dates.len(),
            })
        }
    }

    /// Readies the popup for the next entry after a create-and-continue: the
    /// date moves to the day after the last one created and keeps focus, so a
    /// run is `Ctrl-d` for the next day and `Ctrl-n`/`Ctrl-p` to skip. Tags,
    /// priority and category stay put so the run keeps its classification.
    fn advance_past(&mut self, last_created: NaiveDate, created: usize) {
        self.created_count += created;
        self.title_txt = TextArea::default();

        if let Some(next) = last_created.succ_opt() {
            self.date_txt = TextArea::new(vec![self.date_format.display(&to_utc_midnight(next))]);
            self.date_txt.move_cursor(CursorMove::End);
        }

        self.active_txt = ActiveText::Date;
        self.title_err_msg.clear();
        self.date_err_msg.clear();
        self.priority_err_msg.clear();
        self.category_err_msg.clear();
        self.validate_date();
    }

    pub fn created_count(&self) -> usize {
        self.created_count
    }
}

/// Upper bound on one batch, so a fat-fingered range can't spawn thousands of
/// entries in a single confirm.
const MAX_BATCH_DATES: usize = 366;

const RANGE_SEPARATOR: &str = "..";

/// Parses the date field into the set of days to create. Accepts a single
/// date, a comma-separated list, an inclusive `start..end` range, or any
/// mixture. The result is sorted and de-duplicated.
fn parse_date_batch(format: &DateFormat, text: &str) -> Result<Vec<NaiveDate>, String> {
    let tokens: Vec<&str> = text
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .collect();

    if tokens.is_empty() {
        return Err(String::from("Date cannot be empty"));
    }

    let mut dates = Vec::new();
    for token in tokens {
        match split_range(format, token) {
            Some((start, end)) => {
                if end < start {
                    return Err(format!("'{token}': range ends before it starts"));
                }
                let span = (end - start).num_days() as usize + 1;
                if span > MAX_BATCH_DATES {
                    return Err(format!(
                        "'{token}': {span} days exceeds the {MAX_BATCH_DATES} day limit"
                    ));
                }
                let mut day = start;
                while day <= end {
                    dates.push(day);
                    let Some(next) = day.succ_opt() else { break };
                    day = next;
                }
            }
            None => dates.push(parse_one(format, token)?),
        }
    }

    dates.sort_unstable();
    dates.dedup();

    if dates.len() > MAX_BATCH_DATES {
        return Err(format!(
            "{} dates exceeds the {MAX_BATCH_DATES} entry limit",
            dates.len()
        ));
    }

    Ok(dates)
}

/// Splits `start..end` only when both halves are valid dates, so a date format
/// that itself contains dots still parses as a single date.
fn split_range(format: &DateFormat, token: &str) -> Option<(NaiveDate, NaiveDate)> {
    let (start, end) = token.split_once(RANGE_SEPARATOR)?;
    let start = format.parse(start.trim()).ok()?;
    let end = format.parse(end.trim()).ok()?;
    Some((start, end))
}

fn parse_one(format: &DateFormat, token: &str) -> Result<NaiveDate, String> {
    format
        .parse(token)
        .map_err(|err| format!("'{token}': {err}"))
}

fn to_utc_midnight(date: NaiveDate) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .unwrap()
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
    use super::{
        ActiveText, EntryPopup, MAX_BATCH_DATES, parse_date_batch, tags_to_text, text_to_tags,
    };
    use crate::settings::{DateFormat, Settings};
    use chrono::NaiveDate;
    use tui_textarea::TextArea;

    fn popup_on(date: &str) -> EntryPopup<'static> {
        let settings = Settings::default();
        let mut popup = EntryPopup::new_entry(&settings, "journal");
        popup.date_txt = TextArea::new(vec![date.to_owned()]);
        popup.validate_date();
        popup
    }

    fn day(text: &str) -> NaiveDate {
        DateFormat::default().parse(text).unwrap()
    }

    fn batch(text: &str) -> Vec<NaiveDate> {
        parse_date_batch(&DateFormat::default(), text).unwrap()
    }

    #[test]
    fn a_single_date_parses_to_one_day() {
        assert_eq!(batch("14-03-2026"), vec![day("14-03-2026")]);
    }

    #[test]
    fn a_range_expands_inclusively() {
        assert_eq!(
            batch("01-09-2026..04-09-2026"),
            vec![
                day("01-09-2026"),
                day("02-09-2026"),
                day("03-09-2026"),
                day("04-09-2026"),
            ]
        );
    }

    #[test]
    fn a_range_of_one_day_yields_that_day() {
        assert_eq!(batch("01-09-2026..01-09-2026"), vec![day("01-09-2026")]);
    }

    #[test]
    fn a_range_spans_a_month_boundary() {
        assert_eq!(
            batch("30-08-2026..02-09-2026"),
            vec![
                day("30-08-2026"),
                day("31-08-2026"),
                day("01-09-2026"),
                day("02-09-2026"),
            ]
        );
    }

    #[test]
    fn a_comma_separated_list_parses_each_date() {
        assert_eq!(
            batch("04-09-2026, 01-09-2026"),
            vec![day("01-09-2026"), day("04-09-2026")]
        );
    }

    #[test]
    fn ranges_and_singles_can_be_mixed() {
        assert_eq!(
            batch("01-09-2026..02-09-2026, 07-09-2026"),
            vec![day("01-09-2026"), day("02-09-2026"), day("07-09-2026"),]
        );
    }

    #[test]
    fn overlapping_dates_are_de_duplicated() {
        assert_eq!(
            batch("01-09-2026..03-09-2026, 02-09-2026"),
            vec![day("01-09-2026"), day("02-09-2026"), day("03-09-2026"),]
        );
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        assert_eq!(
            batch("  01-09-2026 .. 02-09-2026 , 07-09-2026  "),
            vec![day("01-09-2026"), day("02-09-2026"), day("07-09-2026"),]
        );
    }

    #[test]
    fn a_trailing_comma_is_ignored() {
        assert_eq!(batch("01-09-2026,"), vec![day("01-09-2026")]);
    }

    #[test]
    fn an_empty_field_is_rejected() {
        assert!(parse_date_batch(&DateFormat::default(), "   ").is_err());
    }

    #[test]
    fn an_unparsable_date_names_the_offending_token() {
        let err = parse_date_batch(&DateFormat::default(), "01-09-2026, nonsense").unwrap_err();
        assert!(err.contains("nonsense"), "{err}");
    }

    #[test]
    fn a_backwards_range_is_rejected() {
        let err = parse_date_batch(&DateFormat::default(), "04-09-2026..01-09-2026").unwrap_err();
        assert!(err.contains("ends before it starts"), "{err}");
    }

    #[test]
    fn a_range_beyond_the_limit_is_rejected() {
        let err = parse_date_batch(&DateFormat::default(), "01-01-2020..01-01-2026").unwrap_err();
        assert!(err.contains(&MAX_BATCH_DATES.to_string()), "{err}");
    }

    #[test]
    fn a_dotted_date_format_still_parses_as_a_single_date() {
        let format = DateFormat::new("DD.MM.YYYY");
        assert_eq!(
            parse_date_batch(&format, "01.09.2026").unwrap(),
            vec![format.parse("01.09.2026").unwrap()]
        );
    }

    #[test]
    fn a_dotted_date_format_still_supports_ranges() {
        let format = DateFormat::new("DD.MM.YYYY");
        assert_eq!(
            parse_date_batch(&format, "01.09.2026..03.09.2026")
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn a_batch_field_validates_clean_on_a_new_entry() {
        let popup = popup_on("01-09-2026..03-09-2026");
        assert!(popup.date_err_msg.is_empty());
    }

    #[test]
    fn a_batch_field_is_rejected_when_editing() {
        let mut popup = popup_on("01-09-2026..03-09-2026");
        popup.is_edit_entry = true;
        popup.validate_date();
        assert_eq!(popup.date_err_msg, "An entry can only have one date");
    }

    #[test]
    fn advance_past_moves_to_the_day_after_the_last_created() {
        let mut popup = popup_on("14-03-2026");
        popup.advance_past(day("14-03-2026"), 1);
        assert_eq!(popup.date_txt.lines()[0], "15-03-2026");
    }

    #[test]
    fn advance_past_moves_past_the_end_of_a_batch() {
        let mut popup = popup_on("01-09-2026..03-09-2026");
        popup.advance_past(day("03-09-2026"), 3);
        assert_eq!(popup.date_txt.lines()[0], "04-09-2026");
    }

    #[test]
    fn advance_past_crosses_a_year_boundary() {
        let mut popup = popup_on("31-12-2026");
        popup.advance_past(day("31-12-2026"), 1);
        assert_eq!(popup.date_txt.lines()[0], "01-01-2027");
    }

    #[test]
    fn advance_past_leaves_focus_on_the_date_field() {
        let mut popup = popup_on("14-03-2026");
        popup.active_txt = ActiveText::Category;
        popup.advance_past(day("14-03-2026"), 1);
        assert_eq!(popup.active_txt, ActiveText::Date);
    }

    #[test]
    fn advance_past_clears_the_title() {
        let mut popup = popup_on("14-03-2026");
        popup.title_txt = TextArea::new(vec!["a title".to_owned()]);
        popup.advance_past(day("14-03-2026"), 1);
        assert_eq!(popup.title_txt.lines()[0], "");
    }

    #[test]
    fn advance_past_keeps_tags_priority_and_category() {
        let mut popup = popup_on("14-03-2026");
        popup.tags_txt = TextArea::new(vec!["alpha, beta".to_owned()]);
        popup.priority_txt = TextArea::new(vec!["2".to_owned()]);
        popup.category_txt = TextArea::new(vec!["work".to_owned()]);

        popup.advance_past(day("14-03-2026"), 1);

        assert_eq!(popup.tags_txt.lines()[0], "alpha, beta");
        assert_eq!(popup.priority_txt.lines()[0], "2");
        assert_eq!(popup.category_txt.lines()[0], "work");
    }

    #[test]
    fn advance_past_counts_every_entry_in_the_batch() {
        let mut popup = popup_on("01-09-2026..03-09-2026");
        assert_eq!(popup.created_count(), 0);
        popup.advance_past(day("03-09-2026"), 3);
        popup.advance_past(day("04-09-2026"), 1);
        assert_eq!(popup.created_count(), 4);
    }

    #[test]
    fn advance_past_clears_a_stale_error_message() {
        let mut popup = popup_on("not a date");
        assert!(!popup.date_err_msg.is_empty());

        popup.advance_past(day("14-03-2026"), 1);

        assert!(popup.date_err_msg.is_empty());
    }

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
