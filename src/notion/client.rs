use std::collections::HashMap;

use anyhow::{Context, anyhow};
use futures::TryStreamExt;
use notionrs::{Client, PaginateExt};
use notionrs_types::object::page::PageProperty;
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

    pub async fn fetch_all_pages(&self, data_source_id: &str) -> anyhow::Result<Vec<PageResponse>> {
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

    pub async fn create_page(
        &self,
        data_source_id: &str,
        properties: HashMap<String, PageProperty>,
        markdown: String,
    ) -> anyhow::Result<PageResponse> {
        self.inner
            .create_page()
            .data_source_id(data_source_id)
            .properties(properties)
            .markdown(markdown)
            .send()
            .await
            .map_err(|err| anyhow!("Failed to create page in {data_source_id}: {err}"))
    }

    pub async fn update_page_properties(
        &self,
        page_id: &str,
        properties: HashMap<String, PageProperty>,
    ) -> anyhow::Result<PageResponse> {
        self.inner
            .update_page()
            .page_id(page_id)
            .properties(properties)
            .send()
            .await
            .map_err(|err| anyhow!("Failed to update page {page_id}: {err}"))
    }

    pub async fn replace_page_markdown(
        &self,
        page_id: &str,
        markdown: String,
    ) -> anyhow::Result<()> {
        self.inner
            .update_page_markdown()
            .page_id(page_id)
            .replace_content_allow_deleting(markdown, true)
            .send()
            .await
            .map_err(|err| anyhow!("Failed to replace markdown for page {page_id}: {err}"))?;
        Ok(())
    }

    pub async fn archive_page(&self, page_id: &str) -> anyhow::Result<()> {
        self.inner
            .update_page()
            .page_id(page_id)
            .in_trash(true)
            .send()
            .await
            .map_err(|err| anyhow!("Failed to archive page {page_id}: {err}"))?;
        Ok(())
    }
}
