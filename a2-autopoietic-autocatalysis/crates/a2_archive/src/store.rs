use std::sync::{Arc, Mutex};

use a2_core::error::A2Result;
use a2_core::id::{GermlineVersion, LineageId, PatchId, TaskId};
use a2_core::protocol::{FitnessRecord, LineageRecord, ModelAttribution};
use a2_core::traits::LineageStore;
use async_trait::async_trait;
use rusqlite::{Connection, params};

use crate::{
    SqliteConnection, deserialize_json, lock_connection, parse_timestamp, schema::init,
    serialize_json, sqlite_error,
};

pub struct SqliteLineageStore {
    connection: SqliteConnection,
}

impl SqliteLineageStore {
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

    fn read_record(row: &rusqlite::Row<'_>) -> A2Result<LineageRecord> {
        let id = row.get::<_, String>(0).map_err(sqlite_error)?;
        let task_id = row.get::<_, String>(1).map_err(sqlite_error)?;
        let patch_id = row.get::<_, String>(2).map_err(sqlite_error)?;
        let parent_germline = row.get::<_, String>(3).map_err(sqlite_error)?;
        let model_attributions_json = row.get::<_, String>(4).map_err(sqlite_error)?;
        let fitness_json = row.get::<_, String>(5).map_err(sqlite_error)?;
        let created_at = row.get::<_, String>(6).map_err(sqlite_error)?;

        Ok(LineageRecord {
            id: deserialize_json::<LineageId>(&id)?,
            task_id: deserialize_json::<TaskId>(&task_id)?,
            patch_id: deserialize_json::<PatchId>(&patch_id)?,
            parent_germline: deserialize_json::<GermlineVersion>(&parent_germline)?,
            model_attributions: deserialize_json::<Vec<ModelAttribution>>(&model_attributions_json)?,
            fitness: deserialize_json::<FitnessRecord>(&fitness_json)?,
            created_at: parse_timestamp(&created_at)?,
        })
    }
}

#[async_trait]
impl LineageStore for SqliteLineageStore {
    async fn record(&self, entry: LineageRecord) -> A2Result<()> {
        let connection = lock_connection(&self.connection)?;

        connection
            .execute(
                "
                INSERT INTO lineage_records (
                    id,
                    task_id,
                    patch_id,
                    parent_germline,
                    model_attributions_json,
                    fitness_json,
                    created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ",
                params![
                    serialize_json(&entry.id)?,
                    serialize_json(&entry.task_id)?,
                    serialize_json(&entry.patch_id)?,
                    serialize_json(&entry.parent_germline)?,
                    serialize_json(&entry.model_attributions)?,
                    serialize_json(&entry.fitness)?,
                    entry.created_at.to_rfc3339(),
                ],
            )
            .map_err(sqlite_error)?;

        Ok(())
    }

    async fn get(&self, id: &LineageId) -> A2Result<Option<LineageRecord>> {
        let connection = lock_connection(&self.connection)?;
        let mut statement = connection
            .prepare(
                "
                SELECT
                    id,
                    task_id,
                    patch_id,
                    parent_germline,
                    model_attributions_json,
                    fitness_json,
                    created_at
                FROM lineage_records
                WHERE id = ?1
                ",
            )
            .map_err(sqlite_error)?;
        let mut rows = statement
            .query(params![serialize_json(id)?])
            .map_err(sqlite_error)?;

        match rows.next().map_err(sqlite_error)? {
            Some(row) => Ok(Some(Self::read_record(row)?)),
            None => Ok(None),
        }
    }

    async fn for_task(&self, task_id: &TaskId) -> A2Result<Vec<LineageRecord>> {
        let connection = lock_connection(&self.connection)?;
        let mut statement = connection
            .prepare(
                "
                SELECT
                    id,
                    task_id,
                    patch_id,
                    parent_germline,
                    model_attributions_json,
                    fitness_json,
                    created_at
                FROM lineage_records
                WHERE task_id = ?1
                ORDER BY created_at ASC
                ",
            )
            .map_err(sqlite_error)?;
        let mut rows = statement
            .query(params![serialize_json(task_id)?])
            .map_err(sqlite_error)?;
        let mut records = Vec::new();

        while let Some(row) = rows.next().map_err(sqlite_error)? {
            records.push(Self::read_record(row)?);
        }

        Ok(records)
    }

    async fn recent(&self, limit: usize) -> A2Result<Vec<LineageRecord>> {
        let connection = lock_connection(&self.connection)?;
        let mut statement = connection
            .prepare(
                "
                SELECT
                    id,
                    task_id,
                    patch_id,
                    parent_germline,
                    model_attributions_json,
                    fitness_json,
                    created_at
                FROM lineage_records
                ORDER BY created_at DESC
                LIMIT ?1
                ",
            )
            .map_err(sqlite_error)?;
        let mut rows = statement
            .query(params![limit as i64])
            .map_err(sqlite_error)?;
        let mut records = Vec::new();

        while let Some(row) = rows.next().map_err(sqlite_error)? {
            records.push(Self::read_record(row)?);
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use a2_core::id::{EvalId, GermlineVersion, LineageId, PatchId, TaskId};
    use a2_core::protocol::{
        FitnessRecord, GermlineFitness, LineageRecord, ModelAttribution, OrganizationalFitness,
        SomaticFitness,
    };
    use chrono::{TimeZone, Utc};
    use rusqlite::Connection;

    use super::SqliteLineageStore;
    use crate::schema::init;
    use a2_core::traits::LineageStore;

    fn sample_lineage_record(
        task_id: TaskId,
        patch_id: PatchId,
        parent_germline: GermlineVersion,
        created_at: chrono::DateTime<Utc>,
    ) -> LineageRecord {
        LineageRecord {
            id: LineageId::new(),
            task_id: task_id.clone(),
            patch_id,
            parent_germline,
            model_attributions: vec![ModelAttribution {
                provider: "openai".into(),
                model: "gpt-5.4".into(),
                tokens_in: 120,
                tokens_out: 48,
            }],
            fitness: FitnessRecord {
                eval_id: EvalId::new(),
                task_id,
                somatic: SomaticFitness {
                    task_completed: true,
                    tests_pass: true,
                    acceptance_met: vec![true, false, true],
                    tokens_used: 168,
                    duration_secs: 2.4,
                },
                germline: Some(GermlineFitness {
                    replay_improvement: 0.3,
                    diversity_contribution: 0.2,
                    regression_clear: true,
                }),
                organizational: Some(OrganizationalFitness {
                    self_host_passes: true,
                    repair_coverage: 0.9,
                    raf_connectivity: 0.85,
                    sentinel_score: 0.92,
                    mission_score: 0.88,
                }),
                evaluated_at: created_at,
            },
            created_at,
        }
    }

    #[tokio::test]
    async fn stores_and_reads_lineage_records() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        init(&connection.lock().unwrap()).unwrap();
        let store = SqliteLineageStore::from_connection(connection).unwrap();

        let created_at = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).single().unwrap();
        let expected = sample_lineage_record(
            TaskId::new(),
            PatchId::new(),
            GermlineVersion::new(),
            created_at,
        );

        store.record(expected.clone()).await.unwrap();

        let actual = store.get(&expected.id).await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&actual).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }

    #[tokio::test]
    async fn filters_by_task_and_orders_recent_records() {
        let connection = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let store = SqliteLineageStore::from_connection(connection).unwrap();

        let shared_task = TaskId::new();
        let first = sample_lineage_record(
            shared_task.clone(),
            PatchId::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 9, 0, 0).single().unwrap(),
        );
        let second = sample_lineage_record(
            shared_task.clone(),
            PatchId::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 10, 0, 0).single().unwrap(),
        );
        let third = sample_lineage_record(
            TaskId::new(),
            PatchId::new(),
            GermlineVersion::new(),
            Utc.with_ymd_and_hms(2026, 4, 1, 11, 0, 0).single().unwrap(),
        );

        store.record(first.clone()).await.unwrap();
        store.record(second.clone()).await.unwrap();
        store.record(third.clone()).await.unwrap();

        let task_records = store.for_task(&shared_task).await.unwrap();
        assert_eq!(task_records.len(), 2);
        assert_eq!(
            serde_json::to_value(&task_records[0]).unwrap(),
            serde_json::to_value(&first).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&task_records[1]).unwrap(),
            serde_json::to_value(&second).unwrap()
        );

        let recent = store.recent(2).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(
            serde_json::to_value(&recent[0]).unwrap(),
            serde_json::to_value(&third).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&recent[1]).unwrap(),
            serde_json::to_value(&second).unwrap()
        );
    }
}
