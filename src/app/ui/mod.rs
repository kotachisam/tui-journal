use std::collections::HashMap;

use backend::DataProvider;
use crossterm::event::KeyCode;
use ratatui::layout::Rect;
pub use themes::Styles;

use self::{
    backstack::Backstack,
    editor::{Editor, EditorMode, MentionFollow},
    entries_list::EntriesList,
    entry_popup::{EntryPopup, EntryPopupInputReturn},
    export_popup::ExportPopup,
    filter_popup::FilterPopup,
    footer::{get_footer_height, render_footer},
    fuzz_find::FuzzFindPopup,
    help_popup::{HelpInputInputReturn, HelpPopup},
    mention_peek_popup::{MentionPeekPopup, MentionPeekReturn},
    msg_box::{MsgBox, MsgBoxActions, MsgBoxType},
    revision_popup::{RevisionPopup, RevisionPopupReturn},
    sort_popup::SortPopup,
    template_popup::{TemplatePopup, TemplatePopupReturn},
};

use super::{
    App,
    keymap::{
        Input, Keymap, get_editor_mode_keymaps, get_entries_list_keymaps, get_global_keymaps,
        get_multi_select_keymaps,
    },
    runner::HandleInputReturnType,
};
use anyhow::Result;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

mod backstack;
mod commands;
mod editor;
mod entries_list;
mod entry_popup;
mod export_handler;
mod export_popup;
mod file_dialog;
mod file_reveal;
mod filter_popup;
mod footer;
mod fuzz_find;
mod help_popup;
mod inline_completer;
mod mention_peek_popup;
mod msg_box;
mod path_completer;
mod popup_handlers;
mod revision_popup;
mod sort_popup;
mod template_popup;
pub mod themes;
mod toast;
pub mod ui_functions;
mod widgets;

pub use commands::UICommand;
pub use msg_box::MsgBoxResult;
pub use toast::Toast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    EntriesList,
    EntryContentTxt,
}

#[derive(Debug, Clone, Copy)]
pub enum ScrollDirection {
    Up,
    Down,
}

fn rect_contains(rect: Rect, column: u16, row: u16) -> bool {
    column >= rect.x
        && column < rect.x + rect.width
        && row >= rect.y
        && row < rect.y + rect.height
}

pub enum Popup<'a> {
    Help(Box<HelpPopup>),
    Entry(Box<EntryPopup<'a>>),
    MsgBox(Box<MsgBox>),
    Export(Box<ExportPopup<'a>>),
    Filter(Box<FilterPopup<'a>>),
    FuzzFind(Box<FuzzFindPopup<'a>>),
    Sort(Box<SortPopup>),
    Template(Box<TemplatePopup>),
    Revision(Box<RevisionPopup>),
    MentionPeek(Box<MentionPeekPopup>),
}

#[derive(Debug, Clone)]
pub enum PopupReturn<T> {
    KeepPopup,
    Cancel,
    Apply(T),
}

enum PopupKind {
    Help,
    Entry,
    MsgBox,
    Export,
    Filter,
    FuzzFind,
    Sort,
    Template,
    Revision,
    MentionPeek,
}

pub struct UIComponents<'a> {
    styles: Styles,
    global_keymaps: Vec<Keymap>,
    entries_list_keymaps: Vec<Keymap>,
    editor_keymaps: Vec<Keymap>,
    multi_select_keymaps: Vec<Keymap>,
    entries_list: EntriesList,
    editor: Editor<'a>,
    popup_stack: Vec<Popup<'a>>,
    pub active_control: ControlType,
    pending_command: Option<UICommand>,
    pub current_toast: Option<Toast>,
    pub pending_exit_after_push: bool,
    pub pending_mention_target: Option<MentionFollow>,
    preview_scrolls: HashMap<u32, u16>,
    backstack: Backstack,
    last_entries_list_rect: Option<Rect>,
    last_editor_rect: Option<Rect>,
}

impl UIComponents<'_> {
    pub fn new(styles: Styles) -> Self {
        let global_keymaps = get_global_keymaps();
        let entries_list_keymaps = get_entries_list_keymaps();
        let editor_keymaps = get_editor_mode_keymaps();
        let multi_select_keymaps = get_multi_select_keymaps();
        let mut entries_list = EntriesList::new();
        let editor = Editor::new();

        let active_control = ControlType::EntriesList;
        entries_list.set_active(true);

        Self {
            styles,
            global_keymaps,
            entries_list_keymaps,
            editor_keymaps,
            multi_select_keymaps,
            entries_list,
            editor,
            popup_stack: Vec::new(),
            active_control,
            pending_command: None,
            current_toast: None,
            pending_exit_after_push: false,
            pending_mention_target: None,
            preview_scrolls: HashMap::new(),
            backstack: Backstack::default(),
            last_entries_list_rect: None,
            last_editor_rect: None,
        }
    }

    pub fn show_toast(&mut self, msg: String) {
        self.current_toast = Some(Toast::new(msg));
    }

    pub fn has_popup(&self) -> bool {
        !self.popup_stack.is_empty()
    }

    pub fn set_current_entry<D: DataProvider>(&mut self, entry_id: Option<u32>, app: &mut App<D>) {
        if let Some(outgoing) = app.current_entry_id {
            self.preview_scrolls
                .insert(outgoing, self.editor.preview_scroll());
        }
        app.current_entry_id = entry_id;
        if let Some(id) = entry_id {
            let entry_index = app.get_active_entries().position(|entry| entry.id == id);
            self.entries_list.state.select(entry_index);
        }

        self.editor.set_current_entry(entry_id, app);

        if let Some(id) = entry_id
            && let Some(stored) = self.preview_scrolls.get(&id).copied()
        {
            self.editor.set_preview_scroll(stored);
        }
    }

    pub fn handle_mouse_scroll<D: DataProvider>(
        &mut self,
        column: u16,
        row: u16,
        direction: ScrollDirection,
        app: &mut App<D>,
    ) {
        let in_entries = self
            .last_entries_list_rect
            .is_some_and(|r| rect_contains(r, column, row));
        let in_editor = self
            .last_editor_rect
            .is_some_and(|r| rect_contains(r, column, row));

        match (in_entries, in_editor, direction) {
            (true, _, ScrollDirection::Up) => self.move_entries_selection(-1, app),
            (true, _, ScrollDirection::Down) => self.move_entries_selection(1, app),
            (_, true, ScrollDirection::Up) => self.editor.scroll_preview_by(-3),
            (_, true, ScrollDirection::Down) => self.editor.scroll_preview_by(3),
            _ => {}
        }
    }

    fn move_entries_selection<D: DataProvider>(&mut self, delta: i32, app: &mut App<D>) {
        let active_ids: Vec<u32> = app.get_active_entries().map(|e| e.id).collect();
        if active_ids.is_empty() {
            return;
        }
        let current_idx = app
            .current_entry_id
            .and_then(|id| active_ids.iter().position(|&x| x == id))
            .unwrap_or(0) as i32;
        let next_idx = (current_idx + delta)
            .clamp(0, (active_ids.len() as i32) - 1) as usize;
        let next_id = active_ids[next_idx];
        if Some(next_id) != app.current_entry_id {
            self.set_current_entry(Some(next_id), app);
        }
    }

    pub async fn handle_mouse_click<D: DataProvider>(
        &mut self,
        column: u16,
        row: u16,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let hit = self
            .editor
            .mention_hitboxes
            .iter()
            .find(|h| h.row == row && column >= h.col_start && column < h.col_end)
            .cloned();
        let Some(hit) = hit else {
            return Ok(HandleInputReturnType::Handled);
        };
        if hit.missing {
            self.show_toast(format!("Entry @id:{} not found", hit.id));
            return Ok(HandleInputReturnType::Handled);
        }
        self.follow_mention(
            MentionFollow {
                id: hit.id,
                anchor: hit.anchor,
            },
            app,
        )
        .await?;
        Ok(HandleInputReturnType::Handled)
    }

    pub fn render_ui<D>(&mut self, f: &mut Frame, app: &App<D>)
    where
        D: DataProvider,
    {
        let footer_height = get_footer_height(f.area().width, self, app);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(2), Constraint::Length(footer_height)].as_ref())
            .split(f.area());

        render_footer(f, chunks[1], self, app);
        self.last_entries_list_rect = None;
        self.last_editor_rect = None;
        if app.state.full_screen {
            match self.active_control {
                ControlType::EntriesList => {
                    self.last_entries_list_rect = Some(chunks[0]);
                    self.entries_list.render_widget(
                        f,
                        chunks[0],
                        app,
                        &self.entries_list_keymaps,
                        &self.styles,
                    );
                }
                ControlType::EntryContentTxt => {
                    self.last_editor_rect = Some(chunks[0]);
                    self.editor.render_widget(
                        f,
                        chunks[0],
                        &self.styles,
                        app.last_search_query.as_deref(),
                        app,
                    );
                }
            }
        } else {
            let entries_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(chunks[0]);
            self.last_entries_list_rect = Some(entries_chunks[0]);
            self.last_editor_rect = Some(entries_chunks[1]);
            self.entries_list.render_widget(
                f,
                entries_chunks[0],
                app,
                &self.entries_list_keymaps,
                &self.styles,
            );
            self.editor.render_widget(
                f,
                entries_chunks[1],
                &self.styles,
                app.last_search_query.as_deref(),
                app,
            );
        }

        self.render_popup(f, app);
    }

    pub fn render_popup<D: DataProvider>(&mut self, f: &mut Frame, app: &App<D>) {
        if let Some(popup) = self.popup_stack.last_mut() {
            match popup {
                Popup::Help(help_popup) => help_popup.render_widget(f, f.area()),
                Popup::Entry(entry_popup) => entry_popup.render_widget(f, f.area(), &self.styles),
                Popup::MsgBox(msg_box) => msg_box.render_widget(f, f.area(), &self.styles),
                Popup::Export(export_popup) => {
                    export_popup.render_widget(f, f.area(), &self.styles)
                }
                Popup::Filter(filter_popup) => {
                    filter_popup.render_widget(f, f.area(), &self.styles)
                }
                Popup::FuzzFind(fuzz_find) => fuzz_find.render_widget(f, f.area(), &self.styles),
                Popup::Sort(sort_popup) => sort_popup.render_widget(f, f.area(), &self.styles),
                Popup::Template(template_popup) => {
                    template_popup.render_widget(f, f.area(), &self.styles)
                }
                Popup::Revision(rev_popup) => rev_popup.render_widget(f, f.area(), &self.styles),
                Popup::MentionPeek(peek_popup) => peek_popup.render_widget(
                    f,
                    f.area(),
                    &self.styles,
                    &app.entries,
                    &app.settings.date_format,
                ),
            }
        }
    }

    pub async fn handle_input<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let result = self.handle_input_inner(input, app).await;
        if let Some(target) = self.editor.pending_mention_follow.take() {
            self.follow_mention(target, app).await?;
        }
        if let Some(id) = self.editor.pending_mention_peek.take() {
            self.open_mention_peek(id);
        }
        result
    }

    async fn handle_input_inner<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        if self.has_popup() {
            return self.handle_popup_input(input, app).await;
        }

        if self.editor.is_preview_mode() && input.key_code == KeyCode::Esc {
            self.editor.toggle_preview();
            return Ok(HandleInputReturnType::Handled);
        }

        if self.editor.is_prioritized() {
            if let Some(key) = self.editor_keymaps.iter().find(|c| &c.key == input) {
                let command_result = key.command.clone().execute(self, app).await?;
                if matches!(command_result, HandleInputReturnType::Handled) {
                    return Ok(command_result);
                }
            }
            let handle_result = self.editor.handle_input_prioritized(input, app)?;
            if matches!(handle_result, HandleInputReturnType::Handled) {
                return Ok(handle_result);
            }
        }

        if self.entries_list.multi_select_mode {
            if let Some(key) = self.multi_select_keymaps.iter().find(|c| &c.key == input) {
                return key.command.to_owned().execute(self, app).await;
            }
            return Ok(HandleInputReturnType::Handled);
        }

        if let Some(cmd) = self
            .global_keymaps
            .iter()
            .find(|keymap| keymap.key == *input)
            .map(|keymap| keymap.command)
        {
            cmd.execute(self, app).await
        } else {
            match self.active_control {
                ControlType::EntriesList => {
                    if let Some(key) = self.entries_list_keymaps.iter().find(|c| &c.key == input) {
                        key.command.clone().execute(self, app).await
                    } else {
                        Ok(HandleInputReturnType::NotFound)
                    }
                }
                ControlType::EntryContentTxt => {
                    if let Some(key) = self.editor_keymaps.iter().find(|c| &c.key == input) {
                        key.command.clone().execute(self, app).await
                    } else {
                        self.editor.handle_input(input, app)
                    }
                }
            }
        }
    }

    async fn handle_popup_input<D: DataProvider>(
        &mut self,
        input: &Input,
        app: &mut App<D>,
    ) -> Result<HandleInputReturnType> {
        let kind = match self.popup_stack.last() {
            Some(Popup::Help(_)) => PopupKind::Help,
            Some(Popup::Entry(_)) => PopupKind::Entry,
            Some(Popup::MsgBox(_)) => PopupKind::MsgBox,
            Some(Popup::Export(_)) => PopupKind::Export,
            Some(Popup::Filter(_)) => PopupKind::Filter,
            Some(Popup::FuzzFind(_)) => PopupKind::FuzzFind,
            Some(Popup::Sort(_)) => PopupKind::Sort,
            Some(Popup::Template(_)) => PopupKind::Template,
            Some(Popup::Revision(_)) => PopupKind::Revision,
            Some(Popup::MentionPeek(_)) => PopupKind::MentionPeek,
            None => return Ok(HandleInputReturnType::NotFound),
        };
        match kind {
            PopupKind::Help => self.handle_help_popup(input),
            PopupKind::Entry => self.handle_entry_popup(input, app).await,
            PopupKind::MsgBox => self.handle_msg_box_popup(input, app).await,
            PopupKind::Export => self.handle_export_popup(input, app).await,
            PopupKind::Filter => self.handle_filter_popup(input, app),
            PopupKind::FuzzFind => self.handle_fuzz_find_popup(input, app),
            PopupKind::Sort => self.handle_sort_popup(input, app),
            PopupKind::Template => self.handle_template_popup(input, app),
            PopupKind::Revision => self.handle_revision_popup(input, app).await,
            PopupKind::MentionPeek => self.handle_mention_peek_popup(input),
        }
    }

    fn set_control_is_active(&mut self, control: ControlType, is_active: bool) {
        match control {
            ControlType::EntriesList => self.entries_list.set_active(is_active),
            ControlType::EntryContentTxt => self.editor.set_active(is_active),
        }
    }

    pub fn change_active_control(&mut self, control: ControlType) {
        if self.active_control == control {
            return;
        }

        self.set_control_is_active(self.active_control, false);
        self.active_control = control;

        self.set_control_is_active(control, true);
    }

    fn start_edit_current_entry(&mut self) -> Result<HandleInputReturnType> {
        if self.entries_list.state.selected().is_none() {
            return Ok(HandleInputReturnType::Handled);
        }

        self.change_active_control(ControlType::EntryContentTxt);

        assert!(!self.editor.is_insert_mode());
        self.editor.set_editor_mode(EditorMode::Insert);
        Ok(HandleInputReturnType::Handled)
    }

    pub fn show_msg_box(
        &mut self,
        msg: MsgBoxType,
        msg_actions: MsgBoxActions,
        pending_cmd: Option<UICommand>,
    ) {
        self.pending_command = pending_cmd;
        let msg_box = MsgBox::new(msg, msg_actions);

        self.popup_stack.push(Popup::MsgBox(Box::new(msg_box)));
    }

    pub fn show_unsaved_msg_box(&mut self, pending_cmd: Option<UICommand>) {
        self.pending_command = pending_cmd;
        let msg =
            MsgBoxType::Question("Do you want to save the changes on the current journal?".into());
        let msg_actions = MsgBoxActions::YesNoCancel;
        let msg_box = MsgBox::new(msg, msg_actions);

        self.popup_stack.push(Popup::MsgBox(Box::new(msg_box)));
    }

    pub fn show_sync_on_exit_msg_box(&mut self, unsynced: usize) {
        self.pending_command = Some(UICommand::QuitAndSync);
        let noun = if unsynced == 1 { "entry" } else { "entries" };
        let msg = MsgBoxType::Question(format!(
            "{unsynced} unsynced {noun}. Push to Notion before exiting?"
        ));
        let msg_actions = MsgBoxActions::YesNoCancel;
        let msg_box = MsgBox::new(msg, msg_actions);

        self.popup_stack.push(Popup::MsgBox(Box::new(msg_box)));
    }

    #[inline]
    pub fn has_unsaved(&self) -> bool {
        self.editor.has_unsaved()
    }

    pub fn show_err_msg(&mut self, err_txt: String) {
        self.show_msg_box(MsgBoxType::Error(err_txt), MsgBoxActions::Ok, None);
    }

    pub fn show_info_msg(&mut self, txt: String) {
        self.show_msg_box(MsgBoxType::Info(txt), MsgBoxActions::Ok, None);
    }

    pub fn open_template_picker(&mut self, templates: Vec<super::templates::Template>) {
        let popup = TemplatePopup::new(templates);
        self.popup_stack.push(Popup::Template(Box::new(popup)));
    }

    pub fn open_mention_peek(&mut self, entry_id: u32) {
        let popup = MentionPeekPopup::new(entry_id);
        self.popup_stack.push(Popup::MentionPeek(Box::new(popup)));
    }

    pub fn open_revision_popup(
        &mut self,
        revisions: Vec<backend::EntryRevision>,
        entry_title: String,
        settings: &crate::settings::Settings,
    ) {
        let popup = RevisionPopup::new(revisions, entry_title, settings);
        self.popup_stack.push(Popup::Revision(Box::new(popup)));
    }

    pub fn show_create_default_templates_prompt(&mut self, dir_display: String) {
        self.pending_command = Some(UICommand::CreateDefaultTemplates);
        let msg = MsgBoxType::Question(format!(
            "No templates found at:\n{dir_display}\n\nCreate default templates there now?"
        ));
        let msg_box = MsgBox::new(msg, MsgBoxActions::YesNoCancel);
        self.popup_stack.push(Popup::MsgBox(Box::new(msg_box)));
    }

    pub fn update_current_entry<D: DataProvider>(&mut self, app: &mut App<D>) {
        if app.get_current_entry().is_none() {
            let first_entry = app.get_active_entries().next().map(|entry| entry.id);
            self.set_current_entry(first_entry, app);
        }
    }
}

