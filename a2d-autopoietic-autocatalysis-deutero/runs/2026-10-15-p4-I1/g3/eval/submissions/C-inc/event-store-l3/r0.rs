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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected {
    Any,
    Missing,
    Exact(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidId { byte: usize },
    Conflict { expected: Expected, actual: Option<u64> },
    Rejected(ReplayError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub account: Account,
    pub tag: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError {
    ZeroInterval,
    InvalidVersion,
    InvalidState,
    TagMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    Snapshot(SnapshotError),
    Tail(ReplayError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stream {
    events: Vec<Event>,
    account: Account,
    snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    snapshot_every: u64,
    streams: std::collections::BTreeMap<String, Stream>,
}

fn initial_account() -> Account {
    Account {
        status: Status::New,
        balance: 0,
        limit: 0,
        version: 0,
    }
}

fn apply_event(account: &mut Account, event: &Event) -> Result<(), ApplyError> {
    if account.version == u64::MAX {
        return Err(ApplyError::VersionOverflow);
    }

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
            match account.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
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
            match account.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
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
            match account.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
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

    account.version += 1;
    Ok(())
}

fn apply_events(start: &Account, events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = start.clone();
    for (index, event) in events.iter().enumerate() {
        apply_event(&mut account, event)
            .map_err(|kind| ReplayError { index, kind })?;
    }
    Ok(account)
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    apply_events(&initial_account(), events)
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Err(StoreError::InvalidId { byte: 0 });
    }
    for (index, &byte) in bytes.iter().enumerate() {
        let valid = if index == 0 {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
        };
        if !valid {
            return Err(StoreError::InvalidId { byte: index });
        }
        if index == 24 {
            return Err(StoreError::InvalidId { byte: 24 });
        }
    }
    if bytes.len() > 24 {
        return Err(StoreError::InvalidId { byte: 24 });
    }
    Ok(())
}

fn snapshot_valid(snapshot: &Snapshot) -> Result<(), SnapshotError> {
    let account = &snapshot.account;
    if account.version == 0 {
        return Err(SnapshotError::InvalidVersion);
    }
    if account.status == Status::New
        || !(1..=1_000_000_000).contains(&account.limit)
        || account.balance < 0
        || account.balance > account.limit
        || (account.status == Status::Closed
            && (account.balance != 0 || account.version < 2))
    {
        return Err(SnapshotError::InvalidState);
    }
    if snapshot.tag != snapshot_tag(account) {
        return Err(SnapshotError::TagMismatch);
    }
    Ok(())
}

impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 {
            return Err(SnapshotError::ZeroInterval);
        }
        Ok(Self {
            snapshot_every,
            streams: std::collections::BTreeMap::new(),
        })
    }

    pub fn append(
        &mut self,
        id: &str,
        expected: Expected,
        events: &[Event],
    ) -> Result<Account, StoreError> {
        validate_id(id)?;

        let existing = self.streams.get(id);
        let actual = existing.map(|stream| stream.account.version);
        let matches = match expected {
            Expected::Any => true,
            Expected::Missing => existing.is_none(),
            Expected::Exact(version) => actual == Some(version),
        };
        if !matches {
            return Err(StoreError::Conflict { expected, actual });
        }

        let start = existing
            .map(|stream| stream.account.clone())
            .unwrap_or_else(initial_account);

        if events.is_empty() {
            return Ok(start);
        }

        let mut account = start;
        let mut new_snapshots = Vec::new();
        for (index, event) in events.iter().enumerate() {
            apply_event(&mut account, event)
                .map_err(|kind| StoreError::Rejected(ReplayError { index, kind }))?;
            if account.version % self.snapshot_every == 0 {
                new_snapshots.push(Snapshot {
                    tag: snapshot_tag(&account),
                    account: account.clone(),
                });
            }
        }

        let stream = self
            .streams
            .entry(id.to_owned())
            .or_insert_with(|| Stream {
                events: Vec::new(),
                account: initial_account(),
                snapshot: None,
            });
        stream.events.extend_from_slice(events);
        stream.account = account.clone();
        if let Some(snapshot) = new_snapshots.pop() {
            stream.snapshot = Some(snapshot);
        }
        Ok(account)
    }

    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|stream| stream.account.clone()))
    }

    pub fn history(&self, id: &str, after: u64) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|stream| {
            stream
                .events
                .iter()
                .enumerate()
                .filter(|(index, _)| (*index as u128) + 1 > after as u128)
                .map(|(_, event)| event.clone())
                .collect()
        }))
    }

    pub fn snapshot(&self, id: &str) -> Result<Option<Snapshot>, StoreError> {
        validate_id(id)?;
        Ok(self
            .streams
            .get(id)
            .and_then(|stream| stream.snapshot.clone()))
    }

    pub fn tail(&self, id: &str) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|stream| {
            let snapshot_version = stream
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.account.version)
                .unwrap_or(0);
            stream
                .events
                .iter()
                .enumerate()
                .filter(|(index, _)| (*index as u128) + 1 > snapshot_version as u128)
                .map(|(_, event)| event.clone())
                .collect()
        }))
    }
}

pub fn snapshot_tag(account: &Account) -> u64 {
    let status_code = match account.status {
        Status::New => 0,
        Status::Open => 0xA5,
        Status::Closed => 0x5A,
    };
    account.version.rotate_left(7)
        ^ (account.limit as u64).wrapping_mul(31)
        ^ (account.balance as u64).rotate_left(17)
        ^ status_code
}

pub fn rebuild(
    snapshot: Option<&Snapshot>,
    tail: &[Event],
) -> Result<Account, RestoreError> {
    match snapshot {
        None => replay(tail).map_err(RestoreError::Tail),
        Some(snapshot) => {
            snapshot_valid(snapshot).map_err(RestoreError::Snapshot)?;
            apply_events(&snapshot.account, tail).map_err(RestoreError::Tail)
        }
    }
}

fn main() {}