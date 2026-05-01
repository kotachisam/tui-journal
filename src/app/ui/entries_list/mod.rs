use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    prelude::Margin,
    style::{Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{
        Block, Borders, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Tabs, Wrap,
    },
};

use backend::DataProvider;

use crate::app::App;
use crate::app::categories;
use crate::{
    app::keymap::Keymap,
    settings::{DatumVisibility, TagVisibility},
};

use super::{Styles, UICommand};

const LIST_INNER_MARGIN: usize = 5;

#[derive(Debug)]
pub struct EntriesList {
    pub state: ListState,
    is_active: bool,
    pub multi_select_mode: bool,
    pub show_entry_ids: bool,
}

impl EntriesList {
    pub fn new() -> Self {
        Self {
            state: ListState::default(),
            is_active: false,
            multi_select_mode: false,
            show_entry_ids: false,
        }
    }

    pub fn toggle_entry_ids(&mut self) {
        self.show_entry_ids = !self.show_entry_ids;
    }

    fn render_list<D: DataProvider>(
        &mut self,
        frame: &mut Frame,
        app: &App<D>,
        area: Rect,
        styles: &Styles,
    ) {
        let jstyles = &styles.journals_list;
        let content_width = (area.width as usize).saturating_sub(LIST_INNER_MARGIN);
        let show_tags = matches!(app.settings.tag_visibility, TagVisibility::Show);
        let tags_default_style: Style = jstyles.tags_default.into();

        let mut lines_count = 0;

        let items: Vec<ListItem> = app
            .get_active_entries()
            .map(|entry| {
                let highlight_selected =
                    self.multi_select_mode && app.selected_entries.contains(&entry.id);

                let title_style = match (self.is_active, highlight_selected) {
                    (_, true) => jstyles.title_selected,
                    (true, _) => jstyles.title_active,
                    (false, _) => jstyles.title_inactive,
                };

                let mut spans = build_title_lines(
                    &entry.title,
                    content_width,
                    title_style.into(),
                    highlight_selected,
                );
                spans.extend(build_date_priority_lines(
                    &app.settings.date_format.display(&entry.date),
                    entry.priority,
                    app.settings.datum_visibility,
                    content_width,
                    jstyles.date_priority.into(),
                    self.show_entry_ids.then_some(entry.id),
                ));
                spans.extend(build_tags_lines(
                    &entry.tags,
                    content_width,
                    show_tags,
                    tags_default_style,
                    |tag| {
                        app.get_color_for_tag(tag)
                            .map(|c| Style::default().bg(c.background).fg(c.foreground))
                            .unwrap_or(tags_default_style)
                    },
                ));

                lines_count += spans.len();
                ListItem::new(spans)
            })
            .collect();

        let items_count = items.len();

        let highlight_style = if self.is_active {
            jstyles.highlight_active
        } else {
            jstyles.highlight_inactive
        };

        let list = List::new(items)
            .block(self.get_list_block(app.filter.is_some(), Some(items_count), styles))
            .highlight_style(highlight_style)
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, area, &mut self.state);

        if lines_count > area.height as usize - 2 && items_count > 0 {
            let avg_item_height = lines_count / items_count;
            self.render_scrollbar(
                frame,
                area,
                self.state.selected().unwrap_or(0),
                items_count,
                avg_item_height,
            );
        }
    }

    fn render_scrollbar(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        pos: usize,
        items_count: usize,
        avg_item_height: usize,
    ) {
        const VIEWPORT_ADJUST: u16 = 4;
        let viewport_len = (area.height / avg_item_height as u16).saturating_sub(VIEWPORT_ADJUST);

        let mut state = ScrollbarState::default()
            .content_length(items_count)
            .viewport_content_length(viewport_len as usize)
            .position(pos);

        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("▲"))
            .end_symbol(Some("▼"))
            .track_symbol(Some(symbols::line::VERTICAL))
            .thumb_symbol(symbols::block::FULL);

        let scroll_area = area.inner(Margin {
            horizontal: 0,
            vertical: 1,
        });

        frame.render_stateful_widget(scrollbar, scroll_area, &mut state);
    }

    fn render_place_holder(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        list_keymaps: &[Keymap],
        has_filter: bool,
        styles: &Styles,
    ) {
        let keys_text: Vec<String> = list_keymaps
            .iter()
            .filter(|keymap| keymap.command == UICommand::CreateEntry)
            .map(|keymap| format!("'{}'", keymap.key))
            .collect();

        let place_holder_text = if self.multi_select_mode {
            String::from("\nNo entries to select")
        } else {
            format!("\n Use {} to create new entry ", keys_text.join(","))
        };

        let place_holder = Paragraph::new(place_holder_text)
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Center)
            .block(self.get_list_block(has_filter, None, styles));

        frame.render_widget(place_holder, area);
    }

    fn get_list_block<'a>(
        &self,
        has_filter: bool,
        entries_len: Option<usize>,
        styles: &Styles,
    ) -> Block<'a> {
        let title = match (self.multi_select_mode, has_filter) {
            (true, true) => "Journals - Multi-Select - Filtered",
            (true, false) => "Journals - Multi-Select",
            (false, true) => "Journals - Filtered",
            (false, false) => "Journals",
        };

        let border_style = match (self.is_active, self.multi_select_mode) {
            (_, true) => styles.journals_list.block_multi_select,
            (true, _) => styles.journals_list.block_active,
            (false, _) => styles.journals_list.block_inactive,
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style);

        match (entries_len, self.state.selected().map(|v| v + 1)) {
            (Some(entries_len), Some(selected)) => {
                block.title_bottom(Line::from(format!("{selected}/{entries_len}")).right_aligned())
            }
            _ => block,
        }
    }

    pub fn render_widget<D: DataProvider>(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        app: &App<D>,
        list_keymaps: &[Keymap],
        styles: &Styles,
    ) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(1)].as_ref())
            .split(area);

        self.render_category_tabs(frame, chunks[0], app);

        let list_area = chunks[1];
        if app.get_active_entries().next().is_none() {
            self.render_place_holder(frame, list_area, list_keymaps, app.filter.is_some(), styles);
        } else {
            self.render_list(frame, app, list_area, styles);
        }
    }

    fn render_category_tabs<D: DataProvider>(&self, frame: &mut Frame, area: Rect, app: &App<D>) {
        let cats = categories::ordered_categories(&app.entries);
        let active_idx = cats
            .iter()
            .position(|c| c == &app.view_category)
            .unwrap_or(0);

        let titles: Vec<Line> = cats
            .iter()
            .map(|c| Line::from(format!(" {} ", capitalize(c))))
            .collect();

        let tabs = Tabs::new(titles)
            .block(Block::default().borders(Borders::ALL).title("Categories"))
            .select(active_idx)
            .style(Style::default())
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        frame.render_widget(tabs, area);
    }

    pub fn set_active(&mut self, active: bool) {
        self.is_active = active;
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn build_title_lines<'a>(
    title: &str,
    content_width: usize,
    style: Style,
    highlight_selected: bool,
) -> Vec<Line<'a>> {
    let mut title = title.to_string();
    if highlight_selected {
        title.insert_str(0, "* ");
    }
    if title.trim().is_empty() {
        return Vec::new();
    }
    textwrap::wrap(&title, content_width)
        .iter()
        .map(|line| Line::from(Span::styled(line.to_string(), style)))
        .collect()
}

fn build_date_priority_lines<'a>(
    date_text: &str,
    priority: Option<u32>,
    datum_visibility: DatumVisibility,
    content_width: usize,
    style: Style,
    entry_id: Option<u32>,
) -> Vec<Line<'a>> {
    let raw_lines: Vec<String> = match (datum_visibility, priority) {
        (DatumVisibility::Show, Some(prio)) => {
            let one_liner = format!("{date_text} | Priority: {prio}");
            if one_liner.len() > content_width {
                vec![date_text.to_owned(), format!("Priority: {prio}")]
            } else {
                vec![one_liner]
            }
        }
        (DatumVisibility::Show, None) => vec![date_text.to_owned()],
        (DatumVisibility::Hide, None) => Vec::new(),
        (DatumVisibility::EmptyLine, None) => vec![String::new()],
        (_, Some(prio)) => vec![format!("Priority: {prio}")],
    };
    let id_badge = entry_id.map(|id| format!("  #{id}"));
    let id_style = Style::default().add_modifier(Modifier::DIM);
    let mut lines: Vec<Line<'a>> = Vec::new();
    let mut id_appended = false;
    for line in raw_lines {
        let needs_id = !id_appended
            && id_badge
                .as_ref()
                .is_some_and(|badge| line.starts_with(date_text)
                    && line.len() + badge.len() <= content_width);
        if needs_id {
            let badge = id_badge.clone().expect("id_badge present");
            lines.push(Line::from(vec![
                Span::styled(line, style),
                Span::styled(badge, id_style),
            ]));
            id_appended = true;
        } else {
            lines.push(Line::from(Span::styled(line, style)));
        }
    }
    lines
}

fn build_tags_lines<'a>(
    tags: &[String],
    content_width: usize,
    show_tags: bool,
    separator_style: Style,
    tag_style: impl Fn(&str) -> Style,
) -> Vec<Line<'a>> {
    if !show_tags || tags.is_empty() {
        return Vec::new();
    }
    const TAGS_SEPARATOR: &str = " | ";

    let mut lines: Vec<Line> = vec![Line::default()];
    for tag in tags {
        let mut last_line = lines.last_mut().unwrap();
        if !last_line.spans.is_empty() {
            if last_line.width() + TAGS_SEPARATOR.len() > content_width {
                lines.push(Line::default());
                last_line = lines.last_mut().unwrap();
            }
            last_line.push_span(Span::styled(TAGS_SEPARATOR, separator_style));
        }

        let span_to_add = Span::styled(tag.to_owned(), tag_style(tag));
        if last_line.width() + tag.len() < content_width {
            last_line.push_span(span_to_add);
        } else {
            lines.push(Line::from(span_to_add));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use ratatui::style::{Color, Style};

    use crate::settings::DatumVisibility;

    use super::{build_date_priority_lines, build_tags_lines, build_title_lines};

    fn plain_style() -> Style {
        Style::default()
    }

    #[test]
    fn title_lines_empty_when_blank() {
        assert!(build_title_lines("   ", 80, plain_style(), false).is_empty());
    }

    #[test]
    fn title_lines_prepends_selection_marker() {
        let lines = build_title_lines("hello", 80, plain_style(), true);
        let rendered: String = lines
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
            .collect();
        assert!(rendered.starts_with("* hello"));
    }

    #[test]
    fn title_lines_wrap_when_too_long() {
        let title = "alpha beta gamma delta epsilon zeta";
        let lines = build_title_lines(title, 12, plain_style(), false);
        assert!(lines.len() > 1, "expected wrap, got {} lines", lines.len());
    }

    #[test]
    fn date_priority_combined_when_fits() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            Some(3),
            DatumVisibility::Show,
            80,
            plain_style(),
            None,
        );
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn date_priority_split_when_too_long() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            Some(3),
            DatumVisibility::Show,
            10,
            plain_style(),
            None,
        );
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn date_priority_hidden_when_no_priority_and_hidden() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            None,
            DatumVisibility::Hide,
            80,
            plain_style(),
            None,
        );
        assert!(lines.is_empty());
    }

    #[test]
    fn date_priority_empty_line_when_configured() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            None,
            DatumVisibility::EmptyLine,
            80,
            plain_style(),
            None,
        );
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn date_priority_appends_id_badge_when_provided() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            None,
            DatumVisibility::Show,
            80,
            plain_style(),
            Some(312),
        );
        assert_eq!(lines.len(), 1);
        let rendered: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, "29-04-2026  #312");
    }

    #[test]
    fn date_priority_omits_id_badge_when_too_narrow() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            None,
            DatumVisibility::Show,
            12,
            plain_style(),
            Some(312),
        );
        assert_eq!(lines.len(), 1);
        let rendered: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(rendered, "29-04-2026");
    }

    #[test]
    fn date_priority_id_badge_skipped_on_priority_only_line() {
        let lines = build_date_priority_lines(
            "29-04-2026",
            Some(3),
            DatumVisibility::Show,
            10,
            plain_style(),
            Some(312),
        );
        assert_eq!(lines.len(), 2);
        let line0: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(line0, "29-04-2026");
        let line1: String = lines[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(line1, "Priority: 3");
    }

    #[test]
    fn tags_empty_when_disabled() {
        let tags = vec!["a".into(), "b".into()];
        assert!(build_tags_lines(&tags, 80, false, plain_style(), |_| plain_style()).is_empty());
    }

    #[test]
    fn tags_empty_when_no_tags() {
        let tags: Vec<String> = vec![];
        assert!(build_tags_lines(&tags, 80, true, plain_style(), |_| plain_style()).is_empty());
    }

    #[test]
    fn tags_fit_on_one_line_when_narrow_enough() {
        let tags = vec!["one".into(), "two".into()];
        let lines = build_tags_lines(&tags, 80, true, plain_style(), |_| plain_style());
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn tags_wrap_when_separator_overflows() {
        // "alpha" (5) + " | " (3) + "beta" (4) = 12 → fits in 13 but not 11
        let tags = vec!["alpha".into(), "beta".into()];
        let lines = build_tags_lines(&tags, 8, true, plain_style(), |_| plain_style());
        assert!(lines.len() >= 2);
    }

    #[test]
    fn tags_use_provided_style_resolver() {
        let tags = vec!["hot".into(), "cold".into()];
        let resolver = |tag: &str| match tag {
            "hot" => Style::default().fg(Color::Red),
            _ => Style::default().fg(Color::Blue),
        };
        let lines = build_tags_lines(&tags, 80, true, plain_style(), resolver);
        let red_count = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.fg == Some(Color::Red))
            .count();
        let blue_count = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.fg == Some(Color::Blue))
            .count();
        assert_eq!(red_count, 1);
        assert_eq!(blue_count, 1);
    }
}
