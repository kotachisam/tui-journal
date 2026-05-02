use tui_textarea::TextArea;

mod clipboard;
mod content;
mod highlight;
mod input;
pub(crate) mod mention;
mod mode;
mod render;

pub use mode::EditorMode;

#[derive(Clone, Debug)]
pub struct MentionHitbox {
    pub row: u16,
    pub col_start: u16,
    pub col_end: u16,
    pub id: u32,
    pub missing: bool,
    pub anchor: Option<String>,
}

pub struct Editor<'a> {
    text_area: TextArea<'a>,
    mode: EditorMode,
    is_active: bool,
    is_dirty: bool,
    has_unsaved: bool,
    preview_scroll: u16,
    show_preview: bool,
    last_wrap_width: Option<u16>,
    mention: Option<mention::MentionState>,
    pub mention_hitboxes: Vec<MentionHitbox>,
    pub pending_mention_follow: Option<MentionFollow>,
    pub pending_mention_peek: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct MentionFollow {
    pub id: u32,
    pub anchor: Option<String>,
}

impl<'a> Editor<'a> {
    pub fn new() -> Editor<'a> {
        let text_area = TextArea::default();

        Editor {
            text_area,
            mode: EditorMode::Normal,
            is_active: false,
            is_dirty: false,
            has_unsaved: false,
            show_preview: true,
            preview_scroll: 0,
            last_wrap_width: None,
            mention: None,
            mention_hitboxes: Vec::new(),
            pending_mention_follow: None,
            pending_mention_peek: None,
        }
    }

    #[inline]
    pub fn is_insert_mode(&self) -> bool {
        self.mode == EditorMode::Insert
    }

    #[inline]
    pub fn is_visual_mode(&self) -> bool {
        self.mode == EditorMode::Visual
    }

    #[inline]
    pub fn is_prioritized(&self) -> bool {
        matches!(self.mode, EditorMode::Insert | EditorMode::Visual)
    }

    pub fn set_preview_scroll(&mut self, line: u16) {
        self.preview_scroll = line;
    }

    pub fn set_active(&mut self, active: bool) {
        if !active && self.is_visual_mode() {
            self.set_editor_mode(EditorMode::Normal);
        }

        self.is_active = active;
    }
}
