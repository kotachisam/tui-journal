use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct NotionSettings {
    #[serde(default)]
    pub database_id: Option<String>,
    #[serde(default)]
    pub sync_mode: SyncMode,
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
