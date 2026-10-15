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

impl Account {
    fn initial() -> Self {
        Self {
            status: Status::New,
            balance: 0,
            limit: 0,
            version: 0,
        }
    }

    fn apply(&mut self, event: &Event) -> Result<(), ApplyError> {
        match event {
            Event::Open { limit } => {
                if !(1..=1_000_000_000).contains(limit) {
                    return Err(ApplyError::InvalidLimit);
                }
                if self.status != Status::New {
                    return Err(ApplyError::AlreadyOpened);
                }
                self.status = Status::Open;
                self.limit = *limit;
            }
            Event::Deposit { amount } => {
                match self.status {
                    Status::New => return Err(ApplyError::NotOpened),
                    Status::Closed => return Err(ApplyError::Closed),
                    Status::Open => {}
                }
                if *amount <= 0 {
                    return Err(ApplyError::InvalidAmount);
                }
                if *amount > self.limit - self.balance {
                    return Err(ApplyError::LimitExceeded);
                }
                self.balance += *amount;
            }
            Event::Withdraw { amount, floor } => {
                match self.status {
                    Status::New => return Err(ApplyError::NotOpened),
                    Status::Closed => return Err(ApplyError::Closed),
                    Status::Open => {}
                }
                if *amount <= 0 {
                    return Err(ApplyError::InvalidAmount);
                }
                if *floor < 0 || *floor > self.limit {
                    return Err(ApplyError::InvalidFloor);
                }
                if *amount > self.balance {
                    return Err(ApplyError::InsufficientFunds);
                }
                if self.balance - *amount < *floor {
                    return Err(ApplyError::FloorViolation);
                }
                self.balance -= *amount;
            }
            Event::Close { expected_version } => {
                match self.status {
                    Status::New => return Err(ApplyError::NotOpened),
                    Status::Closed => return Err(ApplyError::Closed),
                    Status::Open => {}
                }
                if *expected_version != self.version {
                    return Err(ApplyError::CloseVersion {
                        expected: *expected_version,
                        actual: self.version,
                    });
                }
                if self.balance != 0 {
                    return Err(ApplyError::NonZeroBalance);
                }
                self.status = Status::Closed;
            }
        }

        self.version = self
            .version
            .checked_add(1)
            .ok_or(ApplyError::VersionOverflow)?;
        Ok(())
    }
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = Account::initial();

    for (index, event) in events.iter().enumerate() {
        if let Err(kind) = account.apply(event) {
            return Err(ReplayError { index, kind });
        }
    }

    Ok(account)
}