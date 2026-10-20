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

fn lifecycle(status: &Status) -> Result<(), ApplyError> {
    match status {
        Status::New => Err(ApplyError::NotOpened),
        Status::Open => Ok(()),
        Status::Closed => Err(ApplyError::Closed),
    }
}

fn transition(account: &Account, event: &Event) -> Result<Account, ApplyError> {
    let mut next = account.clone();

    match event {
        Event::Open { limit } => {
            if *limit < 1 || *limit > 1_000_000_000 {
                return Err(ApplyError::InvalidLimit);
            }
            if account.status != Status::New {
                return Err(ApplyError::AlreadyOpened);
            }
            next.status = Status::Open;
            next.limit = *limit;
        }
        Event::Deposit { amount } => {
            lifecycle(&account.status)?;
            if *amount <= 0 {
                return Err(ApplyError::InvalidAmount);
            }
            if *amount > account.limit - account.balance {
                return Err(ApplyError::LimitExceeded);
            }
            next.balance = account.balance + *amount;
        }
        Event::Withdraw { amount, floor } => {
            lifecycle(&account.status)?;
            if *amount <= 0 {
                return Err(ApplyError::InvalidAmount);
            }
            if *floor < 0 || *floor > account.limit {
                return Err(ApplyError::InvalidFloor);
            }
            if *amount > account.balance {
                return Err(ApplyError::InsufficientFunds);
            }
            let remaining = account.balance - *amount;
            if remaining < *floor {
                return Err(ApplyError::FloorViolation);
            }
            next.balance = remaining;
        }
        Event::Close { expected_version } => {
            lifecycle(&account.status)?;
            if *expected_version != account.version {
                return Err(ApplyError::CloseVersion {
                    expected: *expected_version,
                    actual: account.version,
                });
            }
            if account.balance != 0 {
                return Err(ApplyError::NonZeroBalance);
            }
            next.status = Status::Closed;
        }
    }

    next.version = account
        .version
        .checked_add(1)
        .ok_or(ApplyError::VersionOverflow)?;
    Ok(next)
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = Account {
        status: Status::New,
        balance: 0,
        limit: 0,
        version: 0,
    };

    for (index, event) in events.iter().enumerate() {
        account = transition(&account, event)
            .map_err(|kind| ReplayError { index, kind })?;
    }

    Ok(account)
}
