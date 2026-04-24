use tui_textarea::CursorMove;

use super::Editor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Normal,
    Insert,
    Visual,
}

impl Editor<'_> {
    pub fn get_editor_mode(&self) -> EditorMode {
        self.mode
    }

    pub fn set_editor_mode(&mut self, mode: EditorMode) {
        let was_previewing = self.show_preview;

        match (self.mode, mode) {
            (EditorMode::Normal, EditorMode::Visual) => {
                self.text_area.start_selection();
            }
            (EditorMode::Visual, EditorMode::Normal | EditorMode::Insert) => {
                self.text_area.cancel_selection();
            }
            _ => {}
        }

        if matches!(mode, EditorMode::Insert | EditorMode::Visual) {
            if matches!(mode, EditorMode::Insert) && was_previewing {
                self.text_area.move_cursor(CursorMove::Bottom);
                self.text_area.move_cursor(CursorMove::End);
            }
            self.show_preview = false;
            self.preview_scroll = 0;
        }

        self.mode = mode;
    }

    pub fn toggle_preview(&mut self) {
        self.show_preview = !self.show_preview;
        self.preview_scroll = 0;
    }

    pub fn is_preview_mode(&self) -> bool {
        self.show_preview
    }
}
