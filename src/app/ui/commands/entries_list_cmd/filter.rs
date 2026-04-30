use std::collections::HashMap;

use crate::app::{App, UIComponents, ui::*};

use backend::{DataProvider, Entry};

use crate::app::ui::commands::{
    CmdResult,
    editor_cmd::{discard_current_content, exec_save_entry_content},
};

pub fn exec_show_filter<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::ShowFilter));
    } else {
        show_filter(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

fn show_filter<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    let tags = app.get_all_tags();
    ui_components
        .popup_stack
        .push(Popup::Filter(Box::new(FilterPopup::new(
            tags,
            app.filter.clone(),
        ))));
}

pub async fn continue_show_filter<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            show_filter(ui_components, app);
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);
            show_filter(ui_components, app);
        }
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_reset_filter<D: DataProvider>(app: &mut App<D>) -> CmdResult {
    app.apply_filter(None);

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_cycle_tag_filter<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::CycleTagFilter));
    } else {
        app.cycle_tags_in_filter();
    }

    Ok(HandleInputReturnType::Handled)
}

pub async fn continue_cycle_tag_filter<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            app.cycle_tags_in_filter();
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);
            app.cycle_tags_in_filter();
        }
    }

    Ok(HandleInputReturnType::Handled)
}

pub fn exec_show_fuzzy_find<D: DataProvider>(
    ui_components: &mut UIComponents,
    app: &mut App<D>,
) -> CmdResult {
    if ui_components.has_unsaved() {
        ui_components.show_unsaved_msg_box(Some(UICommand::ShowFuzzyFind));
    } else {
        show_fuzzy_find(ui_components, app);
    }

    Ok(HandleInputReturnType::Handled)
}

fn show_fuzzy_find<D: DataProvider>(ui_components: &mut UIComponents, app: &mut App<D>) {
    app.last_search_query = None;
    let entries: HashMap<u32, String> = app
        .get_active_entries()
        .map(|entry| (entry.id, build_searchable_text(entry)))
        .collect();
    ui_components
        .popup_stack
        .push(Popup::FuzzFind(Box::new(FuzzFindPopup::new(entries))));
}

fn build_searchable_text(entry: &Entry) -> String {
    const SEPARATOR: &str = " — ";

    let content_flat: String = entry
        .content
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();

    let tags_joined = entry.tags.join(" ");

    [entry.title.trim(), content_flat.trim(), tags_joined.trim()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

pub async fn continue_fuzzy_find<D: DataProvider>(
    ui_components: &mut UIComponents<'_>,
    app: &mut App<D>,
    msg_box_result: MsgBoxResult,
) -> CmdResult {
    match msg_box_result {
        MsgBoxResult::Ok | MsgBoxResult::Cancel => {}
        MsgBoxResult::Yes => {
            exec_save_entry_content(ui_components, app).await?;
            show_fuzzy_find(ui_components, app);
        }
        MsgBoxResult::No => {
            discard_current_content(ui_components, app);
            show_fuzzy_find(ui_components, app);
        }
    }

    Ok(HandleInputReturnType::Handled)
}

#[cfg(test)]
mod searchable_text_tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn entry(title: &str, content: &str, tags: &[&str]) -> Entry {
        Entry::new(
            1,
            Utc.with_ymd_and_hms(2024, 1, 2, 3, 4, 5).unwrap(),
            title.to_owned(),
            content.to_owned(),
            tags.iter().map(|t| (*t).to_owned()).collect(),
            None,
        )
    }

    #[test]
    fn all_three_fields_present() {
        let e = entry("My title", "Some body text", &["rust", "tui"]);
        assert_eq!(
            build_searchable_text(&e),
            "My title — Some body text — rust tui"
        );
    }

    #[test]
    fn empty_title_drops_leading_separator() {
        let e = entry("", "body", &["tag"]);
        assert_eq!(build_searchable_text(&e), "body — tag");
    }

    #[test]
    fn empty_content_drops_middle_separator() {
        let e = entry("title", "", &["tag"]);
        assert_eq!(build_searchable_text(&e), "title — tag");
    }

    #[test]
    fn empty_tags_drops_trailing_separator() {
        let e = entry("title", "body", &[]);
        assert_eq!(build_searchable_text(&e), "title — body");
    }

    #[test]
    fn only_title_no_separator() {
        let e = entry("only", "", &[]);
        assert_eq!(build_searchable_text(&e), "only");
    }

    #[test]
    fn only_content_no_separator() {
        let e = entry("", "only body", &[]);
        assert_eq!(build_searchable_text(&e), "only body");
    }

    #[test]
    fn only_tags_no_separator() {
        let e = entry("", "", &["one", "two"]);
        assert_eq!(build_searchable_text(&e), "one two");
    }

    #[test]
    fn all_empty_returns_empty_string() {
        let e = entry("", "", &[]);
        assert_eq!(build_searchable_text(&e), "");
    }

    #[test]
    fn newlines_and_tabs_in_content_collapse_to_spaces() {
        let e = entry("t", "line one\nline two\tthree", &[]);
        assert_eq!(build_searchable_text(&e), "t — line one line two three");
    }

    #[test]
    fn full_content_is_searchable_past_legacy_120_char_limit() {
        let long = "a".repeat(200) + " needle";
        let e = entry("t", &long, &[]);
        let out = build_searchable_text(&e);
        assert!(out.contains("needle"));
    }
}
