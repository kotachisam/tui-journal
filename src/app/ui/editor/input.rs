use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use tui_textarea::{CursorMove, Scrolling};

use backend::DataProvider;

use crate::app::{
    App, keymap::Input, runner::HandleInputReturnType, ui::commands::ClipboardOperation,
};

use super::{Editor, EditorMode};

impl From<&Input> for KeyEvent {
    fn from(value: &Input) -> Self {
        KeyEvent {
            code: value.key_code,
            modifiers: value.modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }
}

impl Editor<'_> {
    pub fn handle_input_prioritized<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &App<D>,
    ) -> anyhow::Result<HandleInputReturnType> {
        if self.is_insert_mode() {
            // We must handle clipboard operation separately if sync with system clipboard is
            // activated
            if app.settings.sync_os_clipboard {
                let has_ctrl = input.modifiers.contains(KeyModifiers::CONTROL);
                // Keymaps are taken from `text_area` source code
                let handled = match input.key_code {
                    KeyCode::Char('x') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Cut)?;
                        true
                    }
                    KeyCode::Char('c') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Copy)?;
                        true
                    }
                    KeyCode::Char('y') if has_ctrl => {
                        self.exec_os_clipboard(ClipboardOperation::Paste)?;
                        true
                    }
                    _ => false,
                };

                if handled {
                    return Ok(HandleInputReturnType::Handled);
                }
            }

            if self.handle_mention_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.handle_mention_commit(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.try_visual_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            if self.try_snap_vertical_navigation(input) {
                return Ok(HandleInputReturnType::Handled);
            }

            self.dismiss_mention_on_break_char(input);

            let is_at_typed = matches!(input.key_code, KeyCode::Char('@'))
                && input.modifiers == KeyModifiers::NONE;

            let key_event = KeyEvent::from(input);
            if self.text_area.input(key_event) {
                self.is_dirty = true;
                self.refresh_has_unsaved(app);
            }

            if is_at_typed {
                self.maybe_open_mention(app);
            } else if self.mention.is_some() {
                self.update_mention(app);
            }

            return Ok(HandleInputReturnType::Handled);
        }

        Ok(HandleInputReturnType::NotFound)
    }

    pub fn handle_input<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &App<D>,
    ) -> anyhow::Result<HandleInputReturnType> {
        if self.show_preview {
            match input.key_code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.preview_scroll = self.preview_scroll.saturating_add(1);
                    return Ok(HandleInputReturnType::Handled);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1);
                    return Ok(HandleInputReturnType::Handled);
                }
                KeyCode::Char('p') | KeyCode::Esc => {
                    self.toggle_preview();
                    return Ok(HandleInputReturnType::Handled);
                }
                _ => {
                    self.show_preview = false;
                    self.preview_scroll = 0;
                }
            }
        }
        debug_assert!(!self.is_insert_mode());

        if app.get_current_entry().is_none() {
            return Ok(HandleInputReturnType::Handled);
        }

        let sync_os_clipboard = app.settings.sync_os_clipboard;

        if is_default_navigation(input) {
            if !self.try_snap_vertical_navigation(input) {
                let key_event = KeyEvent::from(input);
                self.text_area.input(key_event);
            }
        } else if !self.is_visual_mode()
            || !self.handle_input_visual_only(input, sync_os_clipboard)?
        {
            self.handle_vim_motions(input, sync_os_clipboard)?;
        }

        // Check if the input led the editor to leave the visual mode and make the corresponding UI changes
        if !self.text_area.is_selecting() && self.is_visual_mode() {
            self.set_editor_mode(EditorMode::Normal);
        }

        self.is_dirty = true;
        self.refresh_has_unsaved(app);

        Ok(HandleInputReturnType::Handled)
    }

    /// Handles input specialized for visual mode only like cut and copy
    fn handle_input_visual_only(
        &mut self,
        input: &Input,
        sync_os_clipboard: bool,
    ) -> anyhow::Result<bool> {
        if !input.modifiers.is_empty() {
            return Ok(false);
        }

        match input.key_code {
            KeyCode::Char('d') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Cut)?;
                } else {
                    self.text_area.cut();
                }
                Ok(true)
            }
            KeyCode::Char('y') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Copy)?;
                } else {
                    self.text_area.copy();
                }
                self.set_editor_mode(EditorMode::Normal);
                Ok(true)
            }
            KeyCode::Char('c') => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Copy)?;
                } else {
                    self.text_area.cut();
                }
                self.set_editor_mode(EditorMode::Insert);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn handle_vim_motions(&mut self, input: &Input, sync_os_clipboard: bool) -> anyhow::Result<()> {
        let has_control = input.modifiers.contains(KeyModifiers::CONTROL);

        match (input.key_code, has_control) {
            (KeyCode::Char('h'), false) => {
                self.text_area.move_cursor(CursorMove::Back);
            }
            (KeyCode::Char('j'), false) => {
                self.text_area.move_cursor(CursorMove::Down);
            }
            (KeyCode::Char('k'), false) => {
                self.text_area.move_cursor(CursorMove::Up);
            }
            (KeyCode::Char('l'), false) => {
                self.text_area.move_cursor(CursorMove::Forward);
            }
            (KeyCode::Char('w'), false) | (KeyCode::Char('e'), false) => {
                self.text_area.move_cursor(CursorMove::WordForward);
            }
            (KeyCode::Char('b'), false) => {
                self.text_area.move_cursor(CursorMove::WordBack);
            }
            (KeyCode::Char('^'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
            }
            (KeyCode::Char('$'), false) => {
                self.text_area.move_cursor(CursorMove::End);
            }
            (KeyCode::Char('D'), false) => {
                self.text_area.delete_line_by_end();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
            }
            (KeyCode::Char('C'), false) => {
                self.text_area.delete_line_by_end();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('p'), false) => {
                if sync_os_clipboard {
                    self.exec_os_clipboard(ClipboardOperation::Paste)?;
                } else {
                    self.text_area.paste();
                }
            }
            (KeyCode::Char('u'), false) => {
                self.text_area.undo();
            }
            (KeyCode::Char('r'), true) => {
                self.text_area.redo();
            }
            (KeyCode::Char('x'), false) => {
                self.text_area.delete_next_char();
                self.exec_os_clipboard(ClipboardOperation::Copy)?;
            }
            (KeyCode::Char('i'), false) => self.mode = EditorMode::Insert,
            (KeyCode::Char('a'), false) => {
                self.text_area.move_cursor(CursorMove::Forward);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('A'), false) => {
                self.text_area.move_cursor(CursorMove::End);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('o'), false) => {
                self.text_area.move_cursor(CursorMove::End);
                self.text_area.insert_newline();
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('O'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
                self.text_area.insert_newline();
                self.text_area.move_cursor(CursorMove::Up);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('I'), false) => {
                self.text_area.move_cursor(CursorMove::Head);
                self.mode = EditorMode::Insert;
            }
            (KeyCode::Char('d'), true) => {
                self.text_area.scroll(Scrolling::HalfPageDown);
            }
            (KeyCode::Char('u'), true) => {
                self.text_area.scroll(Scrolling::HalfPageUp);
            }
            (KeyCode::Char('f'), true) => {
                self.text_area.scroll(Scrolling::PageDown);
            }
            (KeyCode::Char('b'), true) => {
                self.text_area.scroll(Scrolling::PageUp);
            }
            _ => {}
        }

        Ok(())
    }

    /// Up on first line / Down on last line snaps to line start/end instead of no-op.
    fn handle_mention_navigation(&mut self, input: &Input) -> bool {
        let Some(mention) = self.mention.as_mut() else {
            return false;
        };
        if !input.modifiers.is_empty() {
            return false;
        }
        match input.key_code {
            KeyCode::Esc => {
                self.mention = None;
                true
            }
            KeyCode::Up => {
                mention.move_up();
                true
            }
            KeyCode::Down => {
                mention.move_down();
                true
            }
            _ => false,
        }
    }

    fn handle_mention_commit(&mut self, input: &Input) -> bool {
        if self.mention.is_none() {
            return false;
        }
        if !matches!(input.key_code, KeyCode::Tab | KeyCode::Enter) {
            return false;
        }
        if !input.modifiers.is_empty() {
            return false;
        }
        let Some(mention) = self.mention.as_ref() else {
            return false;
        };
        let Some(candidate) = mention.selected().cloned() else {
            self.mention = None;
            return false;
        };
        self.commit_mention(candidate.id);
        true
    }

    pub(crate) fn mention_at_cursor(&self) -> Option<u32> {
        let (cursor_line, cursor_col) = self.text_area.cursor();
        let line = self.text_area.lines().get(cursor_line)?;
        let mentions = super::mention::parse_mentions_in_line(line);
        mentions
            .into_iter()
            .find(|m| cursor_col >= m.char_range.start && cursor_col < m.char_range.end)
            .map(|m| m.id)
    }

    fn dismiss_mention_on_break_char(&mut self, input: &Input) {
        if self.mention.is_none() {
            return;
        }
        if let KeyCode::Char(c) = input.key_code
            && super::mention::is_break_char(c)
        {
            self.mention = None;
        }
    }

    fn maybe_open_mention<D: DataProvider>(&mut self, app: &App<D>) {
        let (cursor_line, cursor_col) = self.text_area.cursor();
        if cursor_col == 0 {
            return;
        }
        let at_col = cursor_col - 1;
        let Some(line) = self.text_area.lines().get(cursor_line) else {
            return;
        };
        if !super::mention::should_open_mention(line, at_col) {
            return;
        }
        let mut state = super::mention::MentionState::new(cursor_line, at_col);
        let candidates = super::mention::build_candidates(
            &app.entries,
            app.current_entry_id,
            &app.settings.date_format,
        );
        state.candidates = super::mention::filter_candidates(&candidates, &state.query);
        self.mention = Some(state);
    }

    fn update_mention<D: DataProvider>(&mut self, app: &App<D>) {
        let Some(mention) = self.mention.as_ref() else {
            return;
        };
        let (cursor_line, cursor_col) = self.text_area.cursor();
        if cursor_line != mention.anchor_line || cursor_col <= mention.anchor_col {
            self.mention = None;
            return;
        }
        let Some(line) = self.text_area.lines().get(cursor_line) else {
            self.mention = None;
            return;
        };
        let query: String = line
            .chars()
            .skip(mention.anchor_col + 1)
            .take(cursor_col - mention.anchor_col - 1)
            .collect();
        let candidates = super::mention::build_candidates(
            &app.entries,
            app.current_entry_id,
            &app.settings.date_format,
        );
        let filtered = super::mention::filter_candidates(&candidates, &query);
        if let Some(mention) = self.mention.as_mut() {
            mention.query = query;
            mention.candidates = filtered;
            if mention.selected_idx >= mention.candidates.len() {
                mention.selected_idx = 0;
            }
        }
    }

    fn commit_mention(&mut self, entry_id: u32) {
        let Some(mention) = self.mention.take() else {
            return;
        };
        let token = super::mention::format_mention_token(entry_id);
        let (_, cursor_col) = self.text_area.cursor();
        let chars_to_remove = cursor_col.saturating_sub(mention.anchor_col);

        self.text_area.move_cursor(tui_textarea::CursorMove::Jump(
            mention.anchor_line as u16,
            mention.anchor_col as u16,
        ));
        for _ in 0..chars_to_remove {
            self.text_area.delete_next_char();
        }
        self.text_area.insert_str(&token);
    }

    fn try_visual_navigation(&mut self, input: &Input) -> bool {
        if !self.show_preview || !input.modifiers.is_empty() {
            return false;
        }
        let Some(width) = self.last_wrap_width else {
            return false;
        };
        let delta: i32 = match input.key_code {
            KeyCode::Up => -1,
            KeyCode::Down => 1,
            _ => return false,
        };

        let lines_owned: Vec<String> = self.text_area.lines().to_vec();
        let lines: Vec<&str> = lines_owned.iter().map(String::as_str).collect();
        let (cursor_row, cursor_col) = self.text_area.cursor();
        let rows = super::render::word_wrap_lines(&lines, width);

        let Some((vrow, vcol)) =
            super::render::wrapped_cursor_position(&rows, cursor_row, cursor_col)
        else {
            return false;
        };

        let target_signed = vrow as i32 + delta;
        if target_signed < 0 {
            self.text_area.move_cursor(CursorMove::Top);
            self.text_area.move_cursor(CursorMove::Head);
            return true;
        }
        let (target_row, target_col) =
            super::render::visual_to_source(&rows, target_signed as u16, vcol);
        self.text_area
            .move_cursor(CursorMove::Jump(target_row as u16, target_col as u16));
        true
    }

    fn try_snap_vertical_navigation(&mut self, input: &Input) -> bool {
        if !input.modifiers.is_empty() || self.is_visual_mode() {
            return false;
        }
        match input.key_code {
            KeyCode::Up if self.is_on_first_line() => {
                self.text_area.move_cursor(CursorMove::Head);
                true
            }
            KeyCode::Down if self.is_on_last_line() => {
                self.text_area.move_cursor(CursorMove::End);
                true
            }
            _ => false,
        }
    }

    fn is_on_first_line(&self) -> bool {
        self.text_area.cursor().0 == 0
    }

    fn is_on_last_line(&self) -> bool {
        let (row, _) = self.text_area.cursor();
        row + 1 >= self.text_area.lines().len()
    }
}

fn is_default_navigation(input: &Input) -> bool {
    let has_control = input.modifiers.contains(KeyModifiers::CONTROL);
    let has_alt = input.modifiers.contains(KeyModifiers::ALT);
    match input.key_code {
        KeyCode::Left
        | KeyCode::Right
        | KeyCode::Up
        | KeyCode::Down
        | KeyCode::Home
        | KeyCode::End
        | KeyCode::PageUp
        | KeyCode::PageDown => true,
        KeyCode::Char('p') if has_control || has_alt => true,
        KeyCode::Char('n') if has_control || has_alt => true,
        KeyCode::Char('f') if !has_control && has_alt => true,
        KeyCode::Char('b') if !has_control && has_alt => true,
        KeyCode::Char('e') if has_control || has_alt => true,
        KeyCode::Char('a') if has_control || has_alt => true,
        KeyCode::Char('v') if has_control || has_alt => true,
        _ => false,
    }
}
