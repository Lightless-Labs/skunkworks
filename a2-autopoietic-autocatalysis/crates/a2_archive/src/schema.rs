use a2_core::error::A2Result;
use rusqlite::Connection;

use crate::sqlite_error;

pub fn init(connection: &Connection) -> A2Result<()> {
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS lineage_records (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                patch_id TEXT NOT NULL,
                parent_germline TEXT NOT NULL,
                model_attributions_json TEXT NOT NULL,
                fitness_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_lineage_records_task_created_at
            ON lineage_records(task_id, created_at);

            CREATE INDEX IF NOT EXISTS idx_lineage_records_created_at
            ON lineage_records(created_at DESC);

            CREATE TABLE IF NOT EXISTS promotion_journal (
                id TEXT PRIMARY KEY,
                patch_id TEXT NOT NULL,
                germline_before TEXT NOT NULL,
                germline_after TEXT NOT NULL,
                decision_json TEXT NOT NULL,
                gate_results_json TEXT NOT NULL,
                promoted_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_promotion_journal_promoted_at
            ON promotion_journal(promoted_at DESC);
            ",
        )
        .map_err(sqlite_error)
}
