use backend::DataProvider;

use super::App;
use super::history::{Change, HistoryStack};

impl<D> App<D>
where
    D: DataProvider,
{
    /// Apply undo on entries returning the id of the effected entry.
    pub async fn undo(&mut self) -> anyhow::Result<Option<u32>> {
        match self.history.pop_undo() {
            Some(change) => self.apply_history_change(change, HistoryStack::Redo).await,
            None => Ok(None),
        }
    }

    /// Apply redo on entries returning the id of the effected entry.
    pub async fn redo(&mut self) -> anyhow::Result<Option<u32>> {
        match self.history.pop_redo() {
            Some(change) => self.apply_history_change(change, HistoryStack::Undo).await,
            None => Ok(None),
        }
    }

    async fn apply_history_change(
        &mut self,
        change: Change,
        history_target: HistoryStack,
    ) -> anyhow::Result<Option<u32>> {
        match change {
            Change::AddEntry { id } => {
                log::trace!("History Apply: Add Entry: ID {id}");
                self.delete_entry_intern(id, history_target).await?;
                Ok(None)
            }
            Change::RemoveEntry(entry) => {
                log::trace!("History Apply: Remove Entry: {entry:?}");
                let id = self.restore_entry_intern(*entry, history_target).await?;

                Ok(Some(id))
            }
            Change::EntryAttribute(attr) => {
                log::trace!("History Apply: Change Attributes: {attr:?}");
                self.update_entry_attributes(
                    attr.id,
                    attr.title,
                    attr.date,
                    attr.tags,
                    attr.priority,
                    attr.category,
                    history_target,
                )
                .await?;

                Ok(Some(attr.id))
            }
            Change::EntryContent { id, content } => {
                log::trace!("History Apply: Change Content: ID: {id}");
                self.update_entry_content(id, content, history_target)
                    .await?;
                Ok(Some(id))
            }
        }
    }
}
