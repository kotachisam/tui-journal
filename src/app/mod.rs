use self::{filter::Filter, state::AppState};
use crate::settings::Settings;
use backend::{DataProvider, Entry};
use colored_tags::ColoredTagsManager;
use history::HistoryManager;
use std::collections::HashSet;

pub mod categories;
mod colored_tags;
mod entries;
mod external_editor;
mod filter;
mod filter_sort;
mod history;
mod keymap;
pub(crate) mod persistence;
mod runner;
mod sorter;
pub mod state;
pub mod streak;
mod tags;
pub mod templates;
#[cfg(test)]
mod test;
pub mod ui;
mod undo_redo;

pub use runner::HandleInputReturnType;
pub use runner::run;
pub use runner::run_headless;
pub use ui::UIComponents;

pub struct App<D>
where
    D: DataProvider,
{
    pub data_provide: D,
    pub entries: Vec<Entry>,
    pub current_entry_id: Option<u32>,
    /// Selected entries' IDs in multi-select mode
    pub selected_entries: HashSet<u32>,
    /// Inactive entries' IDs due to not meeting the filter criteria
    pub filtered_out_entries: HashSet<u32>,
    pub settings: Settings,
    pub redraw_after_restore: bool,
    pub filter: Option<Filter>,
    /// Set during the quit flow when the user opts into pushing unsynced
    /// changes to Notion before exiting. Read by the runner after the main
    /// loop returns.
    pub should_push_on_exit: bool,
    pub last_search_query: Option<String>,
    /// The currently-active category tab in the entries list ("journal",
    /// "post", etc.). Persisted across sessions via AppState.
    pub view_category: String,
    state: AppState,
    /// Keeps history of the changes on entries, enabling undo & redo operations
    history: HistoryManager,
    colored_tags: Option<ColoredTagsManager>,
}

impl<D> App<D>
where
    D: DataProvider,
{
    pub fn new(data_provide: D, settings: Settings) -> Self {
        let entries = Vec::new();
        let selected_entries = HashSet::new();
        let filtered_out_entries = HashSet::new();
        let history = HistoryManager::new(settings.history_limit);
        let colored_tags = settings.colored_tags.then(ColoredTagsManager::new);

        Self {
            data_provide,
            entries,
            current_entry_id: None,
            selected_entries,
            filtered_out_entries,
            settings,
            redraw_after_restore: false,
            filter: None,
            should_push_on_exit: false,
            last_search_query: None,
            view_category: backend::DEFAULT_CATEGORY.to_owned(),
            state: Default::default(),
            history,
            colored_tags,
        }
    }

    pub fn set_entries_list_percentage(&mut self, pct: u16) -> bool {
        let clamped = pct.clamp(ui::DIVIDER_MIN_PCT, ui::DIVIDER_MAX_PCT);
        if clamped == self.state.entries_list_percentage {
            return false;
        }
        self.state.entries_list_percentage = clamped;
        true
    }

    pub fn entries_list_percentage(&self) -> u16 {
        self.state.entries_list_percentage
    }
}
