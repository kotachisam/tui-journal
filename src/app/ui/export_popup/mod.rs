use std::{env, path::PathBuf};

use backend::{DataProvider, Entry};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use directories::UserDirs;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Style,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tui_textarea::{CursorMove, TextArea};

use crate::app::{
    App,
    keymap::Input,
    ui::{
        file_dialog::{SaveDialogRequest, save_file_dialog},
        inline_completer::{self, InlineCompleterState},
        path_completer::{
            PathCandidate, PathCompletionProvider, list_directory, parse_path_context,
        },
    },
};

use super::{PopupReturn, Styles, ui_functions::centered_rect_exact_height};

type ExportPopupInputReturn = PopupReturn<(PathBuf, Option<u32>)>;

const FOOTER_MULTI: &str = "Enter: confirm | Esc or <Ctrl-c>: Cancel";
const FOOTER_MULTI_COMPLETING: &str =
    "↑/↓: navigate | Tab/Enter: complete | Esc: dismiss overlay";
const FOOTER_SINGLE: &str =
    "↑/↓/Tab: cycle ext | Ctrl-O: file picker | Enter: confirm | Esc: Cancel";
const FOOTER_SINGLE_COMPLETING: &str =
    "↑/↓: navigate | Tab/Enter: complete | Esc: dismiss overlay";
const FOOTER_MARGINE: u16 = 8;
const DEFAULT_FILE_NAME: &str = "tjournal_export.json";
const CYCLE_EXTENSIONS: &[&str] = &["md", "txt"];
const PATH_SEPARATORS: &[char] = &['/', '\\'];
const MAX_PATH_CANDIDATES: usize = 50;

pub struct ExportPopup<'a> {
    path_txt: TextArea<'a>,
    path_err_msg: String,
    entry_id: Option<u32>,
    paragraph_text: String,
    completion: Option<InlineCompleterState<PathCandidate>>,
}

impl ExportPopup<'_> {
    pub fn create_entry_content<D: DataProvider>(
        entry: &Entry,
        app: &App<D>,
    ) -> anyhow::Result<Self> {
        let mut default_path = if let Some(path) = &app.settings.export.default_path {
            path.clone()
        } else {
            env::current_dir()?
        };

        // Add filename if it's not already defined
        if default_path.extension().is_none() {
            default_path.push(format!("{}.md", entry.title.as_str()));
        }

        let mut path_txt = TextArea::new(vec![default_path.to_string_lossy().to_string()]);
        path_txt.move_cursor(CursorMove::End);

        let paragraph_text = format!("Journal: {}", entry.title.to_owned());

        let mut export_popup = ExportPopup {
            path_txt,
            path_err_msg: String::default(),
            entry_id: Some(entry.id),
            paragraph_text,
            completion: None,
        };

        export_popup.validate_path();
        export_popup.refresh_completion();

        Ok(export_popup)
    }

    pub fn create_multi_select<D: DataProvider>(app: &App<D>) -> anyhow::Result<Self> {
        let mut default_path = if let Some(path) = &app.settings.export.default_path {
            path.clone()
        } else {
            env::current_dir()?
        };

        // Add filename if it's not already defined
        if default_path.extension().is_none() {
            default_path.push(DEFAULT_FILE_NAME);
        }

        let mut path_txt = TextArea::new(vec![default_path.to_string_lossy().to_string()]);
        path_txt.move_cursor(CursorMove::End);

        let paragraph_text = format!(
            "Export the selected {} journals",
            app.selected_entries.len()
        );

        let mut export_popup = ExportPopup {
            path_txt,
            path_err_msg: String::default(),
            entry_id: None,
            paragraph_text,
            completion: None,
        };

        export_popup.validate_path();
        export_popup.refresh_completion();

        Ok(export_popup)
    }

    fn current_path_string(&self) -> String {
        self.path_txt
            .lines()
            .first()
            .cloned()
            .unwrap_or_default()
    }

    fn replace_path(&mut self, new_path: String) {
        self.path_txt = TextArea::new(vec![new_path]);
        self.path_txt.move_cursor(CursorMove::End);
        self.validate_path();
        self.refresh_completion();
    }

    fn is_completing(&self) -> bool {
        self.completion
            .as_ref()
            .is_some_and(|c| !c.candidates.is_empty())
    }

    fn refresh_completion(&mut self) {
        let path_text = self.current_path_string();
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let home = UserDirs::new().map(|d| d.home_dir().to_path_buf());

        let ctx = match parse_path_context(&path_text, home.as_deref(), &cwd) {
            Some(ctx) => ctx,
            None => {
                self.completion = None;
                return;
            }
        };

        let candidates = list_directory(&ctx.parent_dir, &ctx.query, MAX_PATH_CANDIDATES);

        if candidates.is_empty() {
            self.completion = None;
            return;
        }

        let preserved_idx = self.completion.as_ref().map(|c| c.selected_idx).unwrap_or(0);
        let mut state = InlineCompleterState::new(0, 0);
        state.candidates = candidates;
        state.selected_idx = preserved_idx.min(state.candidates.len().saturating_sub(1));
        state.query = ctx.query;
        self.completion = Some(state);
    }

    fn apply_completion(&mut self) {
        let candidate = match self.completion.as_ref().and_then(|c| c.selected().cloned()) {
            Some(c) => c,
            None => return,
        };
        let path_text = self.current_path_string();
        let split_at = path_text
            .rfind(PATH_SEPARATORS)
            .map(|i| i + 1)
            .unwrap_or(0);
        let mut new_path = path_text[..split_at].to_string();
        new_path.push_str(&candidate.name);
        if candidate.is_dir {
            new_path.push('/');
        }
        self.replace_path(new_path);
    }

    fn delete_segment_backward(&mut self) {
        let content = self.current_path_string();
        if content.is_empty() {
            return;
        }
        let trimmed = content.trim_end_matches(PATH_SEPARATORS);
        let new_content = match trimmed.rfind(PATH_SEPARATORS) {
            Some(0) => "/".to_string(),
            Some(idx) => trimmed[..idx].to_string(),
            None => String::new(),
        };
        self.replace_path(new_content);
    }

    fn clear_to_home(&mut self) {
        self.replace_path("~/".to_string());
    }

    fn open_save_dialog(&mut self) {
        let current = self.current_path_string();
        let expanded = expand_tilde(&current);
        let default_pb = if expanded.is_empty() {
            env::current_dir().unwrap_or_default()
        } else {
            PathBuf::from(&expanded)
        };

        let (default_dir, default_name) = match (default_pb.parent(), default_pb.file_name()) {
            (Some(parent), Some(name)) if !parent.as_os_str().is_empty() => {
                (parent.to_path_buf(), name.to_string_lossy().into_owned())
            }
            _ => {
                let cwd = env::current_dir().unwrap_or_default();
                let name = default_pb
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| String::from("export.md"));
                (cwd, name)
            }
        };

        let prompt = if self.is_multi_select_mode() {
            "Save journals as"
        } else {
            "Save journal as"
        };

        let req = SaveDialogRequest {
            prompt,
            default_dir: &default_dir,
            default_name: &default_name,
        };

        match save_file_dialog(&req) {
            Ok(Some(path)) => {
                self.replace_path(path.to_string_lossy().into_owned());
            }
            Ok(None) => {}
            Err(err) => {
                self.path_err_msg = format!("File picker unavailable: {err}");
            }
        }
    }

    fn cycle_extension(&mut self) {
        let line = self
            .path_txt
            .lines()
            .first()
            .cloned()
            .unwrap_or_default();
        if line.is_empty() {
            return;
        }

        let pb = PathBuf::from(&line);
        let stem = pb
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if stem.is_empty() {
            return;
        }
        let parent = pb.parent().map(|p| p.to_path_buf()).filter(|p| !p.as_os_str().is_empty());
        let current_ext = pb
            .extension()
            .and_then(|s| s.to_str())
            .map(|e| e.to_ascii_lowercase());

        let next_ext = match current_ext.as_deref() {
            Some(ext) => match CYCLE_EXTENSIONS.iter().position(|e| *e == ext) {
                Some(idx) => CYCLE_EXTENSIONS[(idx + 1) % CYCLE_EXTENSIONS.len()],
                None => CYCLE_EXTENSIONS[0],
            },
            None => CYCLE_EXTENSIONS[0],
        };

        let new_filename = format!("{stem}.{next_ext}");
        let new_path = match parent {
            Some(p) => p.join(&new_filename).to_string_lossy().into_owned(),
            None => new_filename,
        };

        self.replace_path(new_path);
    }

    fn validate_path(&mut self) {
        let path = self
            .path_txt
            .lines()
            .first()
            .expect("Path Textbox should always have one line");

        if path.is_empty() {
            self.path_err_msg = "Path can't be empty".into();
        } else {
            self.path_err_msg.clear();
        }
    }

    fn is_input_valid(&self) -> bool {
        self.path_err_msg.is_empty()
    }

    fn is_multi_select_mode(&self) -> bool {
        self.entry_id.is_none()
    }

    pub fn render_widget(&mut self, frame: &mut Frame, area: Rect, styles: &Styles) {
        let mut area = centered_rect_exact_height(70, 11, area);

        let completing = self.is_completing();
        let footer_text = match (self.is_multi_select_mode(), completing) {
            (true, true) => FOOTER_MULTI_COMPLETING,
            (true, false) => FOOTER_MULTI,
            (false, true) => FOOTER_SINGLE_COMPLETING,
            (false, false) => FOOTER_SINGLE,
        };

        if area.width < footer_text.chars().count() as u16 + FOOTER_MARGINE {
            area.height += 1;
        }

        let title = if self.is_multi_select_mode() {
            "Export journals"
        } else {
            "Export journal content"
        };

        let block = Block::default().borders(Borders::ALL).title(title);

        frame.render_widget(Clear, area);
        frame.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .horizontal_margin(4)
            .vertical_margin(2)
            .constraints(
                [
                    Constraint::Length(2),
                    Constraint::Length(3),
                    Constraint::Length(1),
                    Constraint::Min(1),
                ]
                .as_ref(),
            )
            .split(area);

        let journal_paragraph =
            Paragraph::new(self.paragraph_text.as_str()).wrap(Wrap { trim: false });
        frame.render_widget(journal_paragraph, chunks[0]);

        if self.path_err_msg.is_empty() {
            let block = Style::from(styles.general.input_block_active);
            let cursor = Style::from(styles.general.input_cursor_active);
            self.path_txt.set_style(block);
            self.path_txt.set_cursor_style(cursor);
            self.path_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title("Path"),
            );
        } else {
            let block = Style::from(styles.general.input_block_invalid);
            let cursor = Style::from(styles.general.input_cursor_invalid);
            self.path_txt.set_style(block);
            self.path_txt.set_cursor_style(cursor);
            self.path_txt.set_block(
                Block::default()
                    .borders(Borders::ALL)
                    .style(block)
                    .title(format!("Path : {}", self.path_err_msg)),
            );
        }

        self.path_txt.set_cursor_line_style(Style::default());

        frame.render_widget(&self.path_txt, chunks[1]);

        let footer = Paragraph::new(footer_text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false });

        frame.render_widget(footer, chunks[3]);

        if let Some(state) = self.completion.as_ref() {
            inline_completer::render_overlay(frame, chunks[1], state, &PathCompletionProvider);
        }
    }

    pub fn handle_input(&mut self, input: &Input) -> ExportPopupInputReturn {
        let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
        let completing = self.is_completing();

        match input.key_code {
            KeyCode::Char('c') if has_ctrl => ExportPopupInputReturn::Cancel,
            KeyCode::Esc => {
                if completing {
                    self.completion = None;
                    ExportPopupInputReturn::KeepPopup
                } else {
                    ExportPopupInputReturn::Cancel
                }
            }
            KeyCode::Enter => {
                if completing {
                    self.apply_completion();
                    ExportPopupInputReturn::KeepPopup
                } else {
                    self.handle_confirm()
                }
            }
            KeyCode::Tab | KeyCode::BackTab if completing => {
                self.apply_completion();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Up if completing => {
                if let Some(c) = self.completion.as_mut() {
                    c.move_up();
                }
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Down if completing => {
                if let Some(c) = self.completion.as_mut() {
                    c.move_down();
                }
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Backspace if has_ctrl => {
                self.delete_segment_backward();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Char('u') if has_ctrl => {
                self.clear_to_home();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Char('o') if has_ctrl => {
                self.open_save_dialog();
                ExportPopupInputReturn::KeepPopup
            }
            KeyCode::Up | KeyCode::Down | KeyCode::Tab | KeyCode::BackTab
                if !self.is_multi_select_mode() =>
            {
                self.cycle_extension();
                ExportPopupInputReturn::KeepPopup
            }
            _ => {
                if self.path_txt.input(KeyEvent::from(input)) {
                    self.validate_path();
                    self.path_txt.scroll((0, i16::MIN));
                    self.refresh_completion();
                }
                ExportPopupInputReturn::KeepPopup
            }
        }
    }

    fn handle_confirm(&mut self) -> ExportPopupInputReturn {
        self.validate_path();
        if !self.is_input_valid() {
            return ExportPopupInputReturn::KeepPopup;
        }

        let raw = self
            .path_txt
            .lines()
            .first()
            .expect("Path Textbox should always have one line")
            .clone();
        let path = PathBuf::from(expand_tilde(&raw));

        ExportPopupInputReturn::Apply((path, self.entry_id))
    }
}

fn expand_tilde(input: &str) -> String {
    if input == "~" {
        return home_dir_string().unwrap_or_else(|| input.to_string());
    }
    if let Some(rest) = input.strip_prefix("~/")
        && let Some(home) = home_dir_string()
    {
        let mut buf = PathBuf::from(home);
        buf.push(rest);
        return buf.to_string_lossy().into_owned();
    }
    input.to_string()
}

fn home_dir_string() -> Option<String> {
    UserDirs::new().map(|d| d.home_dir().to_string_lossy().into_owned())
}
