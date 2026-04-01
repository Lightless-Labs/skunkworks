use thiserror::Error;

use crate::id::{CatalystId, TaskId, WorkcellId};

#[derive(Debug, Error)]
pub enum A2Error {
    #[error("invariant violation: {invariant} — {detail}")]
    InvariantViolation { invariant: String, detail: String },

    #[error("constitutional violation: {clause}")]
    ConstitutionalViolation { clause: String },

    #[error("workcell {0} exceeded budget: {1}")]
    BudgetExceeded(WorkcellId, String),

    #[error("catalyst {0} failed: {1}")]
    CatalystFailure(CatalystId, String),

    #[error("task {0} rejected: {1}")]
    TaskRejected(TaskId, String),

    #[error("promotion rejected: {0}")]
    PromotionRejected(String),

    #[error("membrane denied: {0}")]
    MembraneDenied(String),

    #[error("model provider error: {0}")]
    ProviderError(String),

    #[error("rollback required: {0}")]
    RollbackRequired(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type A2Result<T> = Result<T, A2Error>;
