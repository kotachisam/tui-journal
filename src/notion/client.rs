use anyhow::{Context, anyhow};
use futures::TryStreamExt;
use notionrs::{Client, PaginateExt};
use notionrs_types::prelude::PageResponse;

pub struct NotionClient {
    inner: Client,
    database_id: String,
}

impl NotionClient {
    pub fn new(token: String, database_id: String) -> Self {
        Self {
            inner: Client::new(token),
            database_id,
        }
    }

    pub async fn resolve_data_source_id(&self) -> anyhow::Result<String> {
        let database = self
            .inner
            .retrieve_database()
            .database_id(self.database_id.clone())
            .send()
            .await
            .map_err(|err| anyhow!("Failed to retrieve database {}: {err}", self.database_id))?;

        database
            .data_sources
            .first()
            .map(|reference| reference.id.clone())
            .context("Database has no data sources; ensure the integration has access")
    }

    pub async fn fetch_all_pages(
        &self,
        data_source_id: &str,
    ) -> anyhow::Result<Vec<PageResponse>> {
        self.inner
            .query_data_source()
            .data_source_id(data_source_id)
            .into_stream()
            .try_collect::<Vec<PageResponse>>()
            .await
            .map_err(|err| anyhow!("Failed to query data source {data_source_id}: {err}"))
    }

    pub async fn page_as_markdown(&self, page_id: &str) -> anyhow::Result<String> {
        let response = self
            .inner
            .get_page_markdown()
            .page_id(page_id)
            .send()
            .await
            .map_err(|err| anyhow!("Failed to fetch markdown for page {page_id}: {err}"))?;
        Ok(response.markdown)
    }
}
