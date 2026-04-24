use backend::DataProvider;
use rayon::prelude::*;

use super::App;
use super::filter::{Filter, FilterCriterion, criterion::TagFilterOption};
use super::sorter::{SortCriteria, SortOrder};

impl<D> App<D>
where
    D: DataProvider,
{
    /// Sets and applies the given filter on the entries
    pub fn apply_filter(&mut self, filter: Option<Filter>) {
        self.filter = filter;
        self.update_filtered_out_entries();
    }

    /// Checks if the filter criteria still valid and update them if needed
    pub(super) fn update_filter(&mut self) {
        if self.filter.is_some() {
            let all_tags = self.get_all_tags();
            let filter = self.filter.as_mut().unwrap();

            filter.criteria.retain(|cr| match cr {
                FilterCriterion::Tag(TagFilterOption::Tag(tag)) => all_tags.contains(tag),
                FilterCriterion::Tag(TagFilterOption::NoTags) => !all_tags.is_empty(),
                FilterCriterion::Title(_) => true,
                FilterCriterion::Content(_) => true,
                FilterCriterion::Priority(_) => true,
            });

            if filter.criteria.is_empty() {
                self.filter = None;
            }
        }
    }

    /// Applies filter on the entries and filter out the ones who don't meet the filter's criteria
    pub(super) fn update_filtered_out_entries(&mut self) {
        if let Some(filter) = self.filter.as_ref() {
            self.filtered_out_entries = self
                .entries
                .par_iter()
                .filter(|entry| !filter.check_entry(entry))
                .map(|entry| entry.id)
                .collect();
        } else {
            self.filtered_out_entries.clear();
        }
    }

    pub fn cycle_tags_in_filter(&mut self) {
        let all_tags = self.get_all_tags();
        if all_tags.is_empty() {
            return;
        }
        let all_tags_criteria: Vec<_> = all_tags
            .into_iter()
            .map(TagFilterOption::Tag)
            .chain(std::iter::once(TagFilterOption::NoTags))
            .collect();

        if let Some(mut filter) = self.filter.take() {
            let applied_tags_criteria: Vec<_> = filter
                .criteria
                .iter()
                .filter_map(|c| match c {
                    FilterCriterion::Tag(tag) => Some(tag),
                    _ => None,
                })
                .collect();
            match applied_tags_criteria.len() {
                // No existing tags => apply the first one.
                0 => {
                    filter.criteria.push(FilterCriterion::Tag(
                        all_tags_criteria
                            .into_iter()
                            .next()
                            .expect("Bound check done at the beginning"),
                    ));
                }
                // One tag exist only => Cycle to the next one.
                1 => {
                    let current_tag_criteria = filter
                        .criteria
                        .iter_mut()
                        .find_map(|c| match c {
                            FilterCriterion::Tag(tag) => Some(tag),
                            _ => None,
                        })
                        .expect("Criteria checked for having one Tag only");

                    let tag_pos = all_tags_criteria
                        .iter()
                        .position(|t| t == current_tag_criteria)
                        .unwrap_or_default();

                    let next_index = (tag_pos + 1) % all_tags_criteria.len();
                    *current_tag_criteria = all_tags_criteria.into_iter().nth(next_index).unwrap();
                }
                // Many tags exist => Clean them and apply the first one.
                _ => {
                    filter
                        .criteria
                        .retain(|c| !matches!(c, FilterCriterion::Tag(_)));
                    filter.criteria.push(FilterCriterion::Tag(
                        all_tags_criteria
                            .into_iter()
                            .next()
                            .expect("Bound check done at the beginning"),
                    ));
                }
            }

            self.apply_filter(Some(filter));
        } else {
            // Apply filter with the first criteria
            let mut filter = Filter::default();
            filter.criteria.push(FilterCriterion::Tag(
                all_tags_criteria
                    .into_iter()
                    .next()
                    .expect("Bound check done at the beginning"),
            ));
            self.apply_filter(Some(filter));
        }
    }

    pub fn apply_sort(&mut self, criteria: Vec<SortCriteria>, order: SortOrder) {
        self.state.sorter.set_criteria(criteria);
        self.state.sorter.order = order;

        self.sort_entries();
    }

    pub(super) fn sort_entries(&mut self) {
        self.entries
            .sort_by(|entry1, entry2| self.state.sorter.sort(entry1, entry2));
    }
}
