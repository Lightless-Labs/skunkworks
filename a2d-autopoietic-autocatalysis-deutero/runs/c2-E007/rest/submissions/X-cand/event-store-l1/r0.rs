#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    New,
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub status: Status,
    pub balance: i64,
    pub limit: i64,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Open { limit: i64 },
    Deposit { amount: i64 },
    Withdraw { amount: i64, floor: i64 },
    Close { expected_version: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    VersionOverflow,
    InvalidLimit,
    AlreadyOpened,
    NotOpened,
    Closed,
    InvalidAmount,
    LimitExceeded,
    InvalidFloor,
    InsufficientFunds,
    FloorViolation,
    CloseVersion { expected: u64, actual: u64 },
    NonZeroBalance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    pub index: usize,
    pub kind: ApplyError,
}

fn lifecycle_error(account: &Account) -> Option<ApplyError> {
    match account.status {
        Status::New => Some(ApplyError::NotOpened),
        Status::Open => None,
        Status::Closed => Some(ApplyError::Closed),
    }
}

fn apply_event(account: &mut Account, event: &Event) -> Result<(), ApplyError> {
    match event {
        Event::Open { limit } => {
            if !(1..=1_000_000_000).contains(limit) {
                return Err(ApplyError::InvalidLimit);
            }
            if account.status != Status::New {
                return Err(ApplyError::AlreadyOpened);
            }
            account.status = Status::Open;
            account.limit = *limit;
        }
        Event::Deposit { amount } => {
            if let Some(error) = lifecycle_error(account) {
                return Err(error);
            }
            if *amount <= 0 {
                return Err(ApplyError::InvalidAmount);
            }
            if *amount > account.limit - account.balance {
                return Err(ApplyError::LimitExceeded);
            }
            account.balance += *amount;
        }
        Event::Withdraw { amount, floor } => {
            if let Some(error) = lifecycle_error(account) {
                return Err(error);
            }
            if *amount <= 0 {
                return Err(ApplyError::InvalidAmount);
            }
            if *floor < 0 || *floor > account.limit {
                return Err(ApplyError::InvalidFloor);
            }
            if *amount > account.balance {
                return Err(ApplyError::InsufficientFunds);
            }
            if account.balance - *amount < *floor {
                return Err(ApplyError::FloorViolation);
            }
            account.balance -= *amount;
        }
        Event::Close { expected_version } => {
            if let Some(error) = lifecycle_error(account) {
                return Err(error);
            }
            if *expected_version != account.version {
                return Err(ApplyError::CloseVersion {
                    expected: *expected_version,
                    actual: account.version,
                });
            }
            if account.balance != 0 {
                return Err(ApplyError::NonZeroBalance);
            }
            account.status = Status::Closed;
        }
    }

    account.version = account
        .version
        .checked_add(1)
        .ok_or(ApplyError::VersionOverflow)?;
    Ok(())
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = Account {
        status: Status::New,
        balance: 0,
        limit: 0,
        version: 0,
    };

    for (index, event) in events.iter().enumerate() {
        if let Err(kind) = apply_event(&mut account, event) {
            return Err(ReplayError { index, kind });
        }
    }

    Ok(account)
}