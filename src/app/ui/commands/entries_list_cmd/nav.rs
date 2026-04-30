use crate::app::{App, UIComponents, ui::*};

use backend::DataProvider;

use crate::app::ui::commands::{CmdResult, editor_cmd::exec_save_entry_content};

pub fn exec_select_prev_entry<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::SelectedPrevEntry));
    } else {
        select_prev_entry(1, ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

pub(super) fn select_prev_entry<D: DataProvider>(
    step: usize,
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) {
    let prev_id = ui_components
        .entries_list
        .state
        .selected()
        .map(|index| index.saturating_sub(step))
        .and_then(|prev_index| {
            app.get_active_entries()
                .nth(prev_index)
                .map(|entry| entry.id)
        });

    if prev_id.is_some() {
        ui_components.set_current_entry(prev_id, app);
    }
}

pub async fn continue_select_prev_entry<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            select_prev_entry(1, ui_components, app);
        }
        MsgBoxResult::No => select_prev_entry(1, ui_components, app),
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_select_next_entry<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::SelectedNextEntry));
    } else {
        select_next_entry(1, ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

pub(super) fn select_next_entry<D: DataProvider>(
    step: usize,
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) {
    let next_id = ui_components
        .entries_list
        .state
        .selected()
        .and_then(|index| index.checked_add(step))
        .and_then(|next_index| {
            app.get_active_entries()
                .nth(next_index)
                .or_else(|| app.get_active_entries().next_back())
                .map(|entry| entry.id)
        });

    if next_id.is_some() {
        ui_components.set_current_entry(next_id, app);
    }
}

pub async fn continue_select_next_entry<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            select_next_entry(1, ui_components, app);
        }
        MsgBoxResult::No => select_next_entry(1, ui_components, app),
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn go_to_top_entry<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    let top_id = app.get_active_entries().next().map(|entry| entry.id);

    if top_id.is_some() {
        ui_components.set_current_entry(top_id, app);
    }
}

pub fn go_to_bottom_entry<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    let top_id = app.get_active_entries().next_back().map(|entry| entry.id);

    if top_id.is_some() {
        ui_components.set_current_entry(top_id, app);
    }
}

pub fn page_up_entries<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    let step = app.settings.get_scroll_per_page();

    select_prev_entry(step, ui_components, app);
}

pub fn page_down_entries<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    let step = app.settings.get_scroll_per_page();

    select_next_entry(step, ui_components, app);
}
