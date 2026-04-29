use std::path::PathBuf;

use backend::DataProvider;
use crossterm::event::KeyCode;
pub use themes::Styles;

use self::{
    editor::{Editor, EditorMode},
    entries_list::EntriesList,
    entry_popup::{EntryPopup, EntryPopupInputReturn},
    export_popup::ExportPopup,
    filter_popup::FilterPopup,
    footer::{get_footer_height, render_footer},
    fuzz_find::FuzzFindPopup,
    help_popup::{HelpInputInputReturn, HelpPopup},
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

mod commands;
mod editor;
mod entries_list;
mod entry_popup;
mod export_popup;
mod filter_popup;
mod footer;
mod fuzz_find;
mod help_popup;
mod msg_box;
mod revision_popup;
mod sort_popup;
mod template_popup;
pub mod themes;
mod toast;
pub mod ui_functions;

pub use commands::UICommand;
pub use msg_box::MsgBoxResult;
pub use toast::Toast;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    EntriesList,
    EntryContentTxt,
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
}

#[derive(Debug, Clone)]
pub enum PopupReturn<T> {
    KeepPopup,
    Cancel,
    Apply(T),
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
        }
    }

    pub fn show_toast(&mut self, msg: String) {
        self.current_toast = Some(Toast::new(msg));
    }

    pub fn has_popup(&self) -> bool {
        !self.popup_stack.is_empty()
    }

    pub fn set_current_entry<D: DataProvider>(&mut self, entry_id: Option<u32>, app: &mut App<D>) {
        app.current_entry_id = entry_id;
        if let Some(id) = entry_id {
            let entry_index = app.get_active_entries().position(|entry| entry.id == id);
            self.entries_list.state.select(entry_index);
        }

        self.editor.set_current_entry(entry_id, app);
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
        if app.state.full_screen {
            match self.active_control {
                ControlType::EntriesList => {
                    self.entries_list.render_widget(
                        f,
                        chunks[0],
                        app,
                        &self.entries_list_keymaps,
                        &self.styles,
                    );
                }
                ControlType::EntryContentTxt => {
                    self.editor.render_widget(
                        f,
                        chunks[0],
                        &self.styles,
                        app.last_search_query.as_deref(),
                    );
                }
            }
        } else {
            let entries_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
                .split(chunks[0]);
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
            );
        }

        self.render_popup(f);
    }

    pub fn render_popup(&mut self, f: &mut Frame) {
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
            }
        }
    }

    pub async fn handle_input<D: DataProvider>(
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
        match self.popup_stack.last_mut() {
            Some(popup) => {
                match popup {
                    Popup::Help(help_popup) => {
                        if help_popup.handle_input(input) == HelpInputInputReturn::Close {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                    }
                    Popup::Entry(entry_popup) => {
                        let close_popup = match entry_popup.handle_input(input, app).await? {
                            EntryPopupInputReturn::Cancel => true,
                            EntryPopupInputReturn::KeepPopup => false,
                            EntryPopupInputReturn::AddEntry(entry_id) => {
                                self.set_current_entry(Some(entry_id), app);
                                true
                            }
                            EntryPopupInputReturn::UpdateCurrentEntry => {
                                self.set_current_entry(app.current_entry_id, app);
                                true
                            }
                        };

                        if close_popup {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                    }
                    Popup::MsgBox(msg_box) => match msg_box.handle_input(input) {
                        msg_box::MsgBoxInputResult::Keep => {}
                        msg_box::MsgBoxInputResult::Close(msg_box_result) => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                            if let Some(cmd) = self.pending_command.take() {
                                return cmd.continue_executing(self, app, msg_box_result).await;
                            }
                            if self.pending_exit_after_push {
                                self.pending_exit_after_push = false;
                                return Ok(HandleInputReturnType::ExitApp);
                            }
                        }
                    },
                    Popup::Export(export_popup) => {
                        match export_popup.handle_input(input) {
                            PopupReturn::KeepPopup => {}
                            PopupReturn::Cancel => {
                                self.popup_stack.pop().expect("popup stack isn't empty");
                            }
                            PopupReturn::Apply((path, entry_id)) => {
                                self.handle_export_popup_return(path, entry_id, app).await;
                            }
                        };
                    }
                    Popup::Filter(filter_popup) => match filter_popup.handle_input(input) {
                        PopupReturn::KeepPopup => {}
                        PopupReturn::Cancel => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        PopupReturn::Apply(filter) => {
                            app.apply_filter(filter);
                            self.popup_stack.pop().expect("popup stack isn't empty");

                            // This fixes the bug: Entry will not be highlighted when the result of the filter is one entry only
                            if app.get_active_entries().count() == 1 {
                                let entry_id =
                                    app.get_active_entries().next().map(|entry| entry.id);
                                self.set_current_entry(entry_id, app);
                            }
                        }
                    },
                    Popup::FuzzFind(fuzz_find) => match fuzz_find.handle_input(input) {
                        fuzz_find::FuzzFindReturn::Close => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        fuzz_find::FuzzFindReturn::Commit => {
                            app.last_search_query = fuzz_find
                                .query()
                                .map(|q| q.to_owned())
                                .filter(|q| !q.is_empty());
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        fuzz_find::FuzzFindReturn::SelectEntry(entry_id) => {
                            if entry_id.is_some() {
                                self.set_current_entry(entry_id, app);
                            }
                        }
                    },
                    Popup::Sort(sort_popup) => match sort_popup.handle_input(input) {
                        PopupReturn::KeepPopup => {}
                        PopupReturn::Cancel => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        PopupReturn::Apply(sort_result) => {
                            self.popup_stack.pop().expect("popup stack isn't empty");

                            // Preserve current entry
                            let current_entry_id = app.current_entry_id;

                            app.apply_sort(sort_result.applied_criteria, sort_result.order);

                            self.set_current_entry(current_entry_id, app);
                        }
                    },
                    Popup::Template(template_popup) => match template_popup.handle_input(input) {
                        TemplatePopupReturn::Keep => {}
                        TemplatePopupReturn::Cancel => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        TemplatePopupReturn::Apply(template) => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                            let entry_popup = EntryPopup::from_template(
                                &template,
                                &app.settings,
                                &app.view_category,
                            );
                            self.popup_stack.push(Popup::Entry(Box::new(entry_popup)));
                        }
                    },
                    Popup::Revision(rev_popup) => match rev_popup.handle_input(input) {
                        RevisionPopupReturn::Keep => {}
                        RevisionPopupReturn::Close => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                        }
                        RevisionPopupReturn::Restore(revision) => {
                            self.popup_stack.pop().expect("popup stack isn't empty");
                            let target_id = revision.entry_id;
                            match app.restore_from_revision(target_id, &revision).await {
                                Ok(()) => {
                                    self.set_current_entry(Some(target_id), app);
                                    self.show_info_msg(
                                        "Revision restored. The pre-restore state has been saved as a new entry in the history."
                                            .to_owned(),
                                    );
                                }
                                Err(err) => {
                                    self.show_err_msg(format!("Failed to restore revision: {err}"));
                                }
                            }
                        }
                    },
                }
                Ok(HandleInputReturnType::Handled)
            }
            _ => Ok(HandleInputReturnType::NotFound),
        }
    }

    async fn handle_export_popup_return<D: DataProvider>(
        &mut self,
        path: PathBuf,
        entry_id: Option<u32>,
        app: &mut App<D>,
    ) {
        let (result, confirmation_msg) = if self.entries_list.multi_select_mode {
            let result = app.export_entries(path.clone()).await;
            let msg = format!("Journal(s)  exported to file {}", path.display());

            (result, msg)
        } else {
            let entry_id = entry_id.expect("entry id must have a value in normal mode");
            let result = app.export_entry_content(entry_id, path.clone()).await;
            let msg = format!("Journal content exported to file {}", path.display());

            (result, msg)
        };

        match result {
            Ok(_) => {
                self.popup_stack.pop().expect("popup stack isn't empty");

                if app.settings.export.show_confirmation {
                    self.show_msg_box(MsgBoxType::Info(confirmation_msg), MsgBoxActions::Ok, None);
                }
            }
            Err(err) => {
                self.show_err_msg(format!("Error while exporting journal(s). Err: {err}",));
            }
        };
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
