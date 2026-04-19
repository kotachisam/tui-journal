use backend::EntryDraft;
use chrono::{DateTime, NaiveDate, Utc};
use notionrs_types::object::date::DateOrDateTime;
use notionrs_types::object::rich_text::RichText;
use notionrs_types::prelude::{PageProperty, PageResponse};

pub const NOTION_PROVIDER: &str = "notion";
const DATE_CREATED_PROPERTY: &str = "Date Created";
const TITLE_FALLBACK: &str = "Untitled";

pub fn page_to_draft(page: &PageResponse, markdown: String) -> EntryDraft {
    let title = extract_title(page).unwrap_or_else(|| TITLE_FALLBACK.to_owned());
    let date = extract_date_created(page).unwrap_or_else(|| created_time_to_chrono(page));
    let tags = extract_tags(page);

    let mut draft = EntryDraft::new(date, title, tags, None).with_content(markdown);
    draft.sync_provider = Some(NOTION_PROVIDER.to_owned());
    draft.external_id = Some(page.id.clone());
    draft.last_synced_at = Some(Utc::now());
    draft
}

fn extract_title(page: &PageResponse) -> Option<String> {
    page.properties.iter().find_map(|(_, prop)| match prop {
        PageProperty::Title(title_prop) => Some(join_plain_text(&title_prop.title)),
        _ => None,
    })
}

fn extract_date_created(page: &PageResponse) -> Option<DateTime<Utc>> {
    page.properties
        .iter()
        .find_map(|(name, prop)| match prop {
            PageProperty::Date(date_prop) if name == DATE_CREATED_PROPERTY => date_prop
                .date
                .as_ref()
                .and_then(|inner| inner.start.as_ref())
                .map(date_or_datetime_to_chrono),
            _ => None,
        })
}

fn extract_tags(page: &PageResponse) -> Vec<String> {
    page.properties
        .iter()
        .find_map(|(_, prop)| match prop {
            PageProperty::MultiSelect(ms) => Some(
                ms.multi_select
                    .iter()
                    .map(|option| option.name.clone())
                    .collect(),
            ),
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
