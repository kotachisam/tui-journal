use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct NotionSettings {
    #[serde(default)]
    pub database_id: Option<String>,
    #[serde(default)]
    pub sync_mode: SyncMode,
    #[serde(default)]
    pub mappings: PropertyMappings,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct PropertyMappings {
    /// If set, match the Notion title property by name. If None, the first
    /// property of type `title` is used (Notion schemas have exactly one).
    #[serde(default)]
    pub title_property: Option<String>,
    /// If set, match this Notion date-type property for the entry date.
    /// Defaults to "Date Created" when None, falling back to Notion's
    /// auto-managed created_time if the property is missing.
    #[serde(default)]
    pub date_property: Option<String>,
    /// If set, match this Notion multi_select-type property for tags. If
    /// None, the first multi_select property found is used.
    #[serde(default)]
    pub tags_property: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    LocalOnly,
    Pull,
    Push,
    TwoWay,
}

pub const NOTION_TOKEN_ENV: &str = "NOTION_TOKEN";
pub const NOTION_DATABASE_ID_ENV: &str = "NOTION_DATABASE_ID";

pub fn read_token() -> anyhow::Result<String> {
    std::env::var(NOTION_TOKEN_ENV).map_err(|_| {
        anyhow::anyhow!(
            "{NOTION_TOKEN_ENV} environment variable not set; add it to your shell or .dev.vars"
        )
    })
}

pub fn read_database_id_from_env() -> Option<String> {
    std::env::var(NOTION_DATABASE_ID_ENV).ok()
}
