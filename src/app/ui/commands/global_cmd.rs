use crate::app::{
    App, HandleInputReturnType, UIComponents,
    templates::{create_default_templates, list_templates, templates_dir},
    ui::{help_popup::KeybindingsTabs, *},
};
use crate::settings::notion::SyncMode;

use backend::DataProvider;

use super::{CmdResult, editor_cmd::exec_save_entry_content};

pub fn exec_quit(ui_components: &mut UIComponents) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::Quit));
        Ok(HandleInputReturnType::Handled)
    } else {
        Ok(HandleInputReturnType::ExitApp)
    }
}

pub async fn continue_quit<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => Ok(HandleInputReturnType::Handled),
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            Ok(resolve_exit(ui_components, app).await)
        }
        MsgBoxResult::No => Ok(resolve_exit(ui_components, app).await),
    }
}

/// Decides between an immediate exit and a sync-on-exit prompt. Refreshes
/// entries from the data provider first so the unsynced count reflects
/// current DB state, not any in-memory drift from prior sync operations.
async fn resolve_exit<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> HandleInputReturnType {
    let push_enabled = matches!(
        app.settings.notion.sync_mode,
        SyncMode::Push | SyncMode::TwoWay
    );
    if !push_enabled {
        return HandleInputReturnType::ExitApp;
    }

    if let Err(err) = app.load_entries().await {
        log::warn!("Failed to refresh entries before exit-time sync check: {err}");
    }

    let unsynced = app.unsynced_count();
    if unsynced > 0 {
        ui_components.show_sync_on_exit_msg_box(unsynced);
        HandleInputReturnType::Handled
    } else {
        HandleInputReturnType::ExitApp
    }
}

pub async fn continue_quit_and_sync<D: DataProvider>(
    _ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Yes => {
            app.should_push_on_exit = true;
            Ok(HandleInputReturnType::ExitApp)
        }
        MsgBoxResult::No => Ok(HandleInputReturnType::ExitApp),
        MsgBoxResult::Ok | MsgBoxResult::Cancel => Ok(HandleInputReturnType::Handled),
    }
}

pub fn exec_show_help(ui_components: &mut UIComponents) -> CmdResult {
    let start_tab = match (
        ui_components.active_control,
        ui_components.entries_list.multi_select_mode,
    ) {
        (ControlType::EntriesList, false) => KeybindingsTabs::Global,
        (ControlType::EntriesList, true) => KeybindingsTabs::MultiSelect,
        (ControlType::EntryContentTxt, _) => KeybindingsTabs::Editor,
    };

    ui_components
        .popup_stack
        .push(Popup::Help(Box::new(HelpPopup::new(start_tab))));

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_cycle_forward(ui_components: &mut UIComponents) -> CmdResult {
    let next_control = match ui_components.active_control {
        ControlType::EntriesList => ControlType::EntryContentTxt,
        ControlType::EntryContentTxt => ControlType::EntriesList,
    };

    ui_components.change_active_control(next_control);
    Ok(HandleInputReturnType::Handled)
}

pub fn exec_cycle_backward(ui_components: &mut UIComponents) -> CmdResult {
    let prev_control = match ui_components.active_control {
        ControlType::EntriesList => ControlType::EntryContentTxt,
        ControlType::EntryContentTxt => ControlType::EntriesList,
    };

    ui_components.change_active_control(prev_control);

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_start_edit_content<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    ui_components.start_edit_current_entry()?;
    app.last_search_query = None;

    Ok(HandleInputReturnType::Handled)
}

pub async fn exec_reload_all<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::ReloadAll));
    } else {
        reload_all(ui_components, app).await?;
    }

    Ok(HandleInputReturnType::Handled)
}

async fn reload_all<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> anyhow::Result<()> {
    app.load_entries().await?;
    ui_components.set_current_entry(app.current_entry_id, app);

    Ok(())
}

pub async fn continue_reload_all<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            reload_all(ui_components, app).await?;
        }
        MsgBoxResult::No => reload_all(ui_components, app).await?,
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn exec_undo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::Undo));
    } else {
        undo(ui_components, app).await?;
    }

    Ok(HandleInputReturnType::Handled)
}

async fn undo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> anyhow::Result<()> {
    if let Some(id) = app.undo().await? {
        ui_components.set_current_entry(Some(id), app);
    }

    Ok(())
}

pub async fn continue_undo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            undo(ui_components, app).await?;
        }
        MsgBoxResult::No => undo(ui_components, app).await?,
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn exec_redo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::Redo));
    } else {
        redo(ui_components, app).await?;
    }

    Ok(HandleInputReturnType::Handled)
}

async fn redo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
) -> anyhow::Result<()> {
    if let Some(id) = app.redo().await? {
        ui_components.set_current_entry(Some(id), app);
    }

    Ok(())
}

pub fn exec_show_template_picker(ui_components: &mut UIComponents) -> CmdResult {
    match list_templates() {
        Ok(templates) if !templates.is_empty() => {
            ui_components.open_template_picker(templates);
        }
        Ok(_) => {
            let dir = templates_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "<unknown>".to_owned());
            ui_components.show_create_default_templates_prompt(dir);
        }
        Err(err) => {
            ui_components.show_err_msg(format!("Failed to list templates: {err}"));
        }
    }
    Ok(HandleInputReturnType::Handled)
}

pub fn continue_create_default_templates(
    ui_components: &mut UIComponents,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    if matches!(msg_box_result, MsgBoxResult::Yes) {
        match create_default_templates() {
            Ok(dir) => {
                let path = dir.display().to_string();
                let clipboard_note = match copy_path_to_clipboard(&path) {
                    Ok(()) => {
                        "Path copied to clipboard — paste in Finder (Cmd+Shift+G) to open the folder."
                    }
                    Err(_) => "Clipboard copy failed; path shown above.",
                };
                let msg = format!(
                    "Templates created at:\n{path}\n\n{clipboard_note}\n\nSee README.md in that directory for the format. Copy and edit any .md file to make your own — the filename becomes the template name. Press Shift+N again to pick one.",
                );
                ui_components.show_info_msg(msg);
            }
            Err(err) => ui_components.show_err_msg(format!("Failed to create templates: {err}")),
        }
    }
    Ok(HandleInputReturnType::Handled)
}

fn copy_path_to_clipboard(path: &str) -> Result<(), arboard::Error> {
    let mut clipboard = arboard::Clipboard::new()?;
    clipboard.set_text(path.to_owned())
}

pub async fn continue_redo<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            redo(ui_components, app).await?;
        }
        MsgBoxResult::No => redo(ui_components, app).await?,
    }

    Ok(HandleInputReturnType::Handled)
}
