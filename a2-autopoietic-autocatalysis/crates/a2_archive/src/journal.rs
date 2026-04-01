use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use a2_core::error::A2Result;
use a2_core::id::{GermlineVersion, PatchId, PromotionId};
use a2_core::protocol::{PromotionDecision, PromotionJournalEntry};
use a2_core::traits::PromotionJournal;
use async_trait::async_trait;
use rusqlite::{Connection, params};

use crate::{
    SqliteConnection, deserialize_json, lock_connection, parse_timestamp, schema::init,
    serialize_json, sqlite_error,
};

pub struct SqlitePromotionJournal {
    connection: SqliteConnection,
}

impl SqlitePromotionJournal {
    pub fn new(connection: Connection) -> A2Result<Self> {
        Self::from_connection(Arc::new(Mutex::new(connection)))
    }

    pub fn from_connection(connection: SqliteConnection) -> A2Result<Self> {
        {
            let connection_guard = lock_connection(&connection)?;
            init(&connection_guard)?;
        }

        Ok(Self { connection })
    }

    fn read_entry(row: &rusqlite::Row<'_>) -> A2Result<PromotionJournalEntry> {
        let id = row.get::<_, String>(0).map_err(sqlite_error)?;
        let patch_id = row.get::<_, String>(1).map_err(sqlite_error)?;
        let germline_before = row.get::<_, String>(2).map_err(sqlite_error)?;
        let germline_after = row.get::<_, String>(3).map_err(sqlite_error)?;
        let decision_json = row.get::<_, String>(4).map_err(sqlite_error)?;
        let gate_results_json = row.get::<_, String>(5).map_err(sqlite_error)?;
        let promoted_at = row.get::<_, String>(6).map_err(sqlite_error)?;

        Ok(PromotionJournalEntry {
            id: deserialize_json::<PromotionId>(&id)?,
            patch_id: deserialize_json::<PatchId>(&patch_id)?,
            germline_before: deserialize_json::<GermlineVersion>(&germline_before)?,
            germline_after: deserialize_json::<GermlineVersion>(&germline_after)?,
            decision: deserialize_json::<PromotionDecision>(&decision_json)?,
            gate_results: deserialize_json::<HashMap<String, bool>>(&gate_results_json)?,
            promoted_at: parse_timestamp(&promoted_at)?,
        })
    }
}

#[async_trait]
impl PromotionJournal for SqlitePromotionJournal {
    async fn append(&self, entry: PromotionJournalEntry) -> A2Result<()> {
        let connection = lock_connection(&self.connection)?;

        connection
            .execute(
                "
                INSERT INTO promotion_journal (
                    id,
                    patch_id,
                    germline_before,
                    germline_after,
                    decision_json,
                    gate_results_json,
                    promoted_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    serialize_json(&entry.id)?,
                    serialize_json(&entry.patch_id)?,
                    serialize_json(&entry.germline_before)?,
                    serialize_json(&entry.germline_after)?,
                    serialize_json(&entry.decision)?,
                    serialize_json(&entry.gate_results)?,
                    entry.promoted_at.to_rfc3339(),
                ],
            )
            .map_err(sqlite_error)?;

        Ok(())
    }

    async fn latest(&self) -> A2Result<Option<PromotionJournalEntry>> {
        let connection = lock_connection(&self.connection)?;
        let mut statement = connection
            .prepare(
                "
                SELECT
                    id,
                    patch_id,
                    germline_before,
                    germline_after,
                    decision_json,
                    gate_results_json,
                    promoted_at
                FROM promotion_journal
                ORDER BY promoted_at DESC
                LIMIT 1
                ",
            )
            .map_err(sqlite_error)?;
        let mut rows = statement.query([]).map_err(sqlite_error)?;

        match rows.next().map_err(sqlite_error)? {
            Some(row) => Ok(Some(Self::read_entry(row)?)),
            None => Ok(None),
        }
    }

    async fn history(&self, limit: usize) -> A2Result<Vec<PromotionJournalEntry>> {
        let connection = lock_connection(&self.connection)?;
        let mut statement = connection
            .prepare(
                "
                SELECT
                    id,
                    patch_id,
                    germline_before,
                    germline_after,
                    decision_json,
                    gate_results_json,
                    promoted_at
                FROM promotion_journal
                ORDER BY promoted_at DESC
                LIMIT ?1
                ",
            )
            .map_err(sqlite_error)?;
        let mut rows = statement
            .query(params![limit as i64])
            .map_err(sqlite_error)?;
        let mut entries = Vec::new();

        while let Some(row) = rows.next().map_err(sqlite_error)? {
            entries.push(Self::read_entry(row)?);
        }

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use a2_core::id::{GermlineVersion, PatchId, PromotionId};
    use a2_core::protocol::{MutationScope, PromotionDecision, PromotionJournalEntry};
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;

    use super::SqlitePromotionJournal;
    use a2_core::traits::PromotionJournal;

    fn sample_entry(
        patch_id: PatchId,
        germline_before: GermlineVersion,
        germline_after: GermlineVersion,
        promoted_at: chrono::DateTime<Utc>,
    ) -> PromotionJournalEntry {
        PromotionJournalEntry {
            id: PromotionId::new(),
            patch_id,
            germline_before,
            germline_after,
            decision: PromotionDecision::PromoteGermline {
                mutation_scope: MutationScope::Catalyst,
            },
            gate_results: HashMap::from([("tests".into(), true), ("constitutional".into(), true)]),
            promoted_at,
        }
    }

    #[tokio::test]
    async fn appends_and_reads_latest_entry() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let journal = SqlitePromotionJournal::from_connection(connection).unwrap();

        let expected = sample_entry(
            PatchId::new(),
            GermlineVersion::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 13, 0, 0).single().unwrap(),
        );

        journal.append(expected.clone()).await.unwrap();

        let actual = journal.latest().await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[tokio::test]
    async fn returns_history_in_reverse_chronological_order() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let journal = SqlitePromotionJournal::from_connection(connection).unwrap();

        let first = sample_entry(
            PatchId::new(),
            GermlineVersion::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 8, 0, 0).single().unwrap(),
        );
        let second = sample_entry(
            PatchId::new(),
            GermlineVersion::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0).single().unwrap(),
        );
        let third = sample_entry(
            PatchId::new(),
            GermlineVersion::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 10, 0, 0).single().unwrap(),
        );

        journal.append(first.clone()).await.unwrap();
        journal.append(second.clone()).await.unwrap();
        journal.append(third.clone()).await.unwrap();

        let history = journal.history(2).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(
            serde_json::to_value(&history[0]).unwrap(),
            serde_json::to_value(&third).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&history[1]).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
    }
}
