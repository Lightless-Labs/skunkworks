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

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = Account {
        status: Status::New,
        balance: 0,
        limit: 0,
        version: 0,
    };

    for (index, event) in events.iter().enumerate() {
        let kind = match event {
            Event::Open { limit } => {
                if !(1..=1_000_000_000).contains(limit) {
                    Some(ApplyError::InvalidLimit)
                } else if account.status != Status::New {
                    Some(ApplyError::AlreadyOpened)
                } else {
                    account.status = Status::Open;
                    account.limit = *limit;
                    None
                }
            }
            Event::Deposit { amount } => match account.status {
                Status::New => Some(ApplyError::NotOpened),
                Status::Closed => Some(ApplyError::Closed),
                Status::Open => {
                    if *amount <= 0 {
                        Some(ApplyError::InvalidAmount)
                    } else if *amount > account.limit - account.balance {
                        Some(ApplyError::LimitExceeded)
                    } else {
                        account.balance += *amount;
                        None
                    }
                }
            },
            Event::Withdraw { amount, floor } => match account.status {
                Status::New => Some(ApplyError::NotOpened),
                Status::Closed => Some(ApplyError::Closed),
                Status::Open => {
                    if *amount <= 0 {
                        Some(ApplyError::InvalidAmount)
                    } else if *floor < 0 || *floor > account.limit {
                        Some(ApplyError::InvalidFloor)
                    } else if *amount > account.balance {
                        Some(ApplyError::InsufficientFunds)
                    } else if account.balance - *amount < *floor {
                        Some(ApplyError::FloorViolation)
                    } else {
                        account.balance -= *amount;
                        None
                    }
                }
            },
            Event::Close { expected_version } => match account.status {
                Status::New => Some(ApplyError::NotOpened),
                Status::Closed => Some(ApplyError::Closed),
                Status::Open => {
                    if *expected_version != account.version {
                        Some(ApplyError::CloseVersion {
                            expected: *expected_version,
                            actual: account.version,
                        })
                    } else if account.balance != 0 {
                        Some(ApplyError::NonZeroBalance)
                    } else {
                        account.status = Status::Closed;
                        None
                    }
                }
            },
        };

        if let Some(kind) = kind {
            return Err(ReplayError { index, kind });
        }

        account.version += 1;
    }

    Ok(account)
}

fn main() {}