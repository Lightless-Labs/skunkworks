use std::io;
use std::sync::{Arc, Mutex, MutexGuard};

use a2_core::error::{A2Error, A2Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::Serialize;
use serde::de::DeserializeOwned;

pub mod journal;
pub mod schema;
pub mod store;

pub use journal::SqlitePromotionJournal;
pub use schema::init;
pub use store::SqliteLineageStore;

pub type SqliteConnection = Arc<Mutex<Connection>>;

pub(crate) fn lock_connection(connection: &SqliteConnection) -> A2Result<MutexGuard<'_, Connection>> {
    connection
        .lock()
        .map_err(|_| A2Error::Io(io::Error::other("sqlite connection mutex poisoned")))
}

pub(crate) fn parse_timestamp(raw: &str) -> A2Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(io_error)
}

pub(crate) fn serialize_json<T: Serialize>(value: &T) -> A2Result<String> {
    serde_json::to_string(value).map_err(Into::into)
}

pub(crate) fn deserialize_json<T: DeserializeOwned>(raw: &str) -> A2Result<T> {
    serde_json::from_str(raw).map_err(Into::into)
}

pub(crate) fn sqlite_error(error: rusqlite::Error) -> A2Error {
    io_error(error)
}

pub(crate) fn io_error(error: impl std::error::Error + Send + Sync + 'static) -> A2Error {
    A2Error::Io(io::Error::other(error))
}
