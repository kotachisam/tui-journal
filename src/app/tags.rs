use std::collections::BTreeSet;

use backend::DataProvider;

use super::App;
use super::colored_tags::TagColors;

impl<D> App<D>
where
    D: DataProvider,
{
    pub fn get_all_tags(&self) -> Vec<String> {
        let mut tags = BTreeSet::new();

        for tag in self.entries.iter().flat_map(|entry| &entry.tags) {
            tags.insert(tag);
        }

        tags.into_iter().map(String::from).collect()
    }

    /// Updates the colors tags mapping, assigning colors to new one and removing the non existing
    /// tags from the colors map.
    pub(super) fn update_colored_tags(&mut self) {
        if self.colored_tags.is_none() {
            return;
        }

        let tags = { self.get_all_tags() };
        if let Some(colored_tags) = self.colored_tags.as_mut() {
            colored_tags.update_tags(tags);
        }
    }

    /// Gets the matching color for the giving tag if colored tags are enabled and tag exists.
    pub fn get_color_for_tag(&self, tag: &str) -> Option<TagColors> {
        self.colored_tags
            .as_ref()
            .and_then(|c| c.get_tag_color(tag))
    }
}
