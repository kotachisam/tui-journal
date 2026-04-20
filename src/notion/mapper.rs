use backend::EntryDraft;
use chrono::{DateTime, NaiveDate, Utc};
use notionrs_types::object::date::DateOrDateTime;
use notionrs_types::object::rich_text::RichText;
use notionrs_types::prelude::{PageProperty, PageResponse};

use crate::settings::notion::PropertyMappings;

pub const NOTION_PROVIDER: &str = "notion";
const DEFAULT_DATE_PROPERTY: &str = "Date Created";
const TITLE_FALLBACK: &str = "Untitled";
const EMPTY_BLOCK_MARKER: &str = "<empty-block/>";
const UNKNOWN_BLOCK_MARKER: &str = "<unknown>";

pub fn page_to_draft(
    page: &PageResponse,
    markdown: String,
    mappings: &PropertyMappings,
) -> EntryDraft {
    let title = extract_title(page, mappings.title_property.as_deref())
        .unwrap_or_else(|| TITLE_FALLBACK.to_owned());
    let date = extract_date(page, mappings.date_property.as_deref())
        .unwrap_or_else(|| created_time_to_chrono(page));
    let tags = extract_tags(page, mappings.tags_property.as_deref());
    let content = sanitize_markdown(&markdown);

    let mut draft = EntryDraft::new(date, title, tags, None).with_content(content);
    draft.sync_provider = Some(NOTION_PROVIDER.to_owned());
    draft.external_id = Some(page.id.clone());
    draft.last_synced_at = Some(Utc::now());
    draft
}

fn sanitize_markdown(raw: &str) -> String {
    raw.lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed != EMPTY_BLOCK_MARKER && trimmed != UNKNOWN_BLOCK_MARKER
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_title(page: &PageResponse, configured_name: Option<&str>) -> Option<String> {
    page.properties.iter().find_map(|(name, prop)| match prop {
        PageProperty::Title(title_prop)
            if configured_name.is_none_or(|wanted| wanted == name) =>
        {
            Some(join_plain_text(&title_prop.title))
        }
        _ => None,
    })
}

fn extract_date(page: &PageResponse, configured_name: Option<&str>) -> Option<DateTime<Utc>> {
    let wanted = configured_name.unwrap_or(DEFAULT_DATE_PROPERTY);
    page.properties
        .iter()
        .find_map(|(name, prop)| match prop {
            PageProperty::Date(date_prop) if name == wanted => date_prop
                .date
                .as_ref()
                .and_then(|inner| inner.start.as_ref())
                .map(date_or_datetime_to_chrono),
            _ => None,
        })
}

fn extract_tags(page: &PageResponse, configured_name: Option<&str>) -> Vec<String> {
    page.properties
        .iter()
        .find_map(|(name, prop)| match prop {
            PageProperty::MultiSelect(ms)
                if configured_name.is_none_or(|wanted| wanted == name) =>
            {
                Some(
                    ms.multi_select
                        .iter()
                        .map(|option| option.name.clone())
                        .collect(),
                )
            }
            _ => None,
        })
        .unwrap_or_default()
}

fn join_plain_text(rich: &[RichText]) -> String {
    rich.iter()
        .map(|rt| match rt {
            RichText::Text { plain_text, .. }
            | RichText::Mention { plain_text, .. }
            | RichText::Equation { plain_text, .. } => plain_text.as_str(),
        })
        .collect()
}

fn date_or_datetime_to_chrono(value: &DateOrDateTime) -> DateTime<Utc> {
    match value {
        DateOrDateTime::DateTime(odt) => offset_datetime_to_chrono(*odt),
        DateOrDateTime::Date(d) => {
            NaiveDate::from_ymd_opt(d.year(), u32::from(u8::from(d.month())), u32::from(d.day()))
                .and_then(|nd| nd.and_hms_opt(0, 0, 0))
                .map(|ndt| ndt.and_utc())
                .unwrap_or_else(Utc::now)
        }
    }
}

fn created_time_to_chrono(page: &PageResponse) -> DateTime<Utc> {
    offset_datetime_to_chrono(page.created_time)
}

fn offset_datetime_to_chrono(odt: time::OffsetDateTime) -> DateTime<Utc> {
    DateTime::<Utc>::from_timestamp(odt.unix_timestamp(), odt.nanosecond()).unwrap_or_else(Utc::now)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_json(properties_json: &str, created_time: &str) -> String {
        format!(
            r#"{{
                "object": "page",
                "id": "18f65ee7-b159-80d2-8b99-f4dc7da6507d",
                "created_time": "{created_time}",
                "last_edited_time": "2026-04-19T10:00:00.000Z",
                "created_by": {{"object": "user", "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"}},
                "last_edited_by": {{"object": "user", "id": "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"}},
                "cover": null,
                "icon": null,
                "parent": {{"type": "workspace", "workspace": true}},
                "archived": false,
                "properties": {properties_json},
                "url": "https://www.notion.so/test",
                "public_url": null,
                "developer_survey": null,
                "request_id": null,
                "in_trash": false,
                "is_locked": false,
                "is_archived": false
            }}"#
        )
    }

    fn parse(json: &str) -> PageResponse {
        serde_json::from_str(json).expect("fixture should deserialize")
    }

    #[test]
    fn populates_sync_metadata() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []}
        }"#;
        let page = parse(&page_json(props, "2025-01-15T08:00:00.000Z"));

        let draft = page_to_draft(&page, "body".to_owned(), &PropertyMappings::default());

        assert_eq!(draft.sync_provider.as_deref(), Some(NOTION_PROVIDER));
        assert_eq!(
            draft.external_id.as_deref(),
            Some("18f65ee7-b159-80d2-8b99-f4dc7da6507d")
        );
        assert!(draft.last_synced_at.is_some());
    }

    #[test]
    fn title_joins_rich_text_segments() {
        let props = r#"{
            "Name": {
                "id": "title", "type": "title",
                "title": [
                    {"type": "text", "text": {"content": "Morning ", "link": null},
                     "annotations": {"bold": false, "italic": false, "strikethrough": false, "underline": false, "code": false, "color": "default"},
                     "plain_text": "Morning ", "href": null},
                    {"type": "text", "text": {"content": "Pages", "link": null},
                     "annotations": {"bold": true, "italic": false, "strikethrough": false, "underline": false, "code": false, "color": "default"},
                     "plain_text": "Pages", "href": null}
                ]
            }
        }"#;
        let page = parse(&page_json(props, "2025-01-15T08:00:00.000Z"));

        let draft = page_to_draft(&page, String::new(), &PropertyMappings::default());

        assert_eq!(draft.title, "Morning Pages");
    }

    #[test]
    fn missing_title_falls_back_to_untitled() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []}
        }"#;
        let page = parse(&page_json(props, "2025-01-15T08:00:00.000Z"));

        let draft = page_to_draft(&page, String::new(), &PropertyMappings::default());

        assert_eq!(draft.title, "");
    }

    #[test]
    fn date_created_wins_over_created_time() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []},
            "Date Created": {
                "id": "TPaA", "type": "date",
                "date": {"start": "2025-01-05T00:00:00.000Z", "end": null, "time_zone": null}
            }
        }"#;
        let page = parse(&page_json(props, "2026-04-19T12:00:00.000Z"));

        let draft = page_to_draft(&page, String::new(), &PropertyMappings::default());

        assert_eq!(draft.date.format("%Y-%m-%d").to_string(), "2025-01-05");
    }

    #[test]
    fn falls_back_to_created_time_when_date_created_missing() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []}
        }"#;
        let page = parse(&page_json(props, "2025-06-01T00:00:00.000Z"));

        let draft = page_to_draft(&page, String::new(), &PropertyMappings::default());

        assert_eq!(draft.date.format("%Y-%m-%d").to_string(), "2025-06-01");
    }

    #[test]
    fn tags_preserve_notion_order() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []},
            "Tags": {
                "id": "KiI", "type": "multi_select",
                "multi_select": [
                    {"id": "1", "name": "🟢 Aligned", "color": "green"},
                    {"id": "2", "name": "🧠 Curious", "color": "green"},
                    {"id": "3", "name": "🪨 Grounded", "color": "green"}
                ]
            }
        }"#;
        let page = parse(&page_json(props, "2025-01-15T08:00:00.000Z"));

        let draft = page_to_draft(&page, String::new(), &PropertyMappings::default());

        assert_eq!(
            draft.tags,
            vec![
                "🟢 Aligned".to_owned(),
                "🧠 Curious".to_owned(),
                "🪨 Grounded".to_owned(),
            ]
        );
    }

    #[test]
    fn custom_date_property_is_respected() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []},
            "Entry Date": {
                "id": "x", "type": "date",
                "date": {"start": "2024-11-20T00:00:00.000Z", "end": null, "time_zone": null}
            }
        }"#;
        let page = parse(&page_json(props, "2026-04-19T12:00:00.000Z"));
        let mappings = PropertyMappings {
            date_property: Some("Entry Date".to_owned()),
            ..Default::default()
        };

        let draft = page_to_draft(&page, String::new(), &mappings);

        assert_eq!(draft.date.format("%Y-%m-%d").to_string(), "2024-11-20");
    }

    #[test]
    fn custom_tags_property_filters_by_name() {
        let props = r#"{
            "Name": {"id": "title", "type": "title", "title": []},
            "Categories": {
                "id": "a", "type": "multi_select",
                "multi_select": [{"id": "1", "name": "journal", "color": "blue"}]
            },
            "Moods": {
                "id": "b", "type": "multi_select",
                "multi_select": [{"id": "2", "name": "focused", "color": "yellow"}]
            }
        }"#;
        let page = parse(&page_json(props, "2025-01-15T08:00:00.000Z"));
        let mappings = PropertyMappings {
            tags_property: Some("Moods".to_owned()),
            ..Default::default()
        };

        let draft = page_to_draft(&page, String::new(), &mappings);

        assert_eq!(draft.tags, vec!["focused".to_owned()]);
    }

    #[test]
    fn sanitize_markdown_strips_empty_block_lines() {
        let raw = "paragraph one\n<empty-block/>\nparagraph two\n  <empty-block/>  \nparagraph three";

        let cleaned = sanitize_markdown(raw);

        assert_eq!(cleaned, "paragraph one\nparagraph two\nparagraph three");
    }

    #[test]
    fn sanitize_markdown_strips_unknown_block_lines() {
        let raw = "hello\n<unknown>\ngoodbye";

        let cleaned = sanitize_markdown(raw);

        assert_eq!(cleaned, "hello\ngoodbye");
    }

    #[test]
    fn sanitize_markdown_leaves_legitimate_html_alone() {
        let raw = "inline <strong>bold</strong> text\n<blockquote>quote</blockquote>";

        let cleaned = sanitize_markdown(raw);

        assert_eq!(cleaned, raw);
    }
}
