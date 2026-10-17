use std::collections::BTreeMap;

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

fn initial_account() -> Account {
    Account {
        status: Status::New,
        balance: 0,
        limit: 0,
        version: 0,
    }
}

fn apply_event(account: &mut Account, event: &Event) -> Result<(), ApplyError> {
    let next_version = account
        .version
        .checked_add(1)
        .ok_or(ApplyError::VersionOverflow)?;

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

    account.version = next_version;
    Ok(())
}

fn apply_events(account: &mut Account, events: &[Event]) -> Result<(), ReplayError> {
    for (index, event) in events.iter().enumerate() {
        apply_event(account, event).map_err(|kind| ReplayError { index, kind })?;
    }
    Ok(())
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = initial_account();
    apply_events(&mut account, events)?;
    Ok(account)
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
    Conflict {
        expected: Expected,
        actual: Option<u64>,
    },
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
    history: Vec<Event>,
    account: Account,
    snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    snapshot_every: u64,
    streams: BTreeMap<String, Stream>,
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Err(StoreError::InvalidId { byte: 0 });
    }

    for (index, byte) in bytes.iter().enumerate() {
        if index >= 24 {
            return Err(StoreError::InvalidId { byte: 24 });
        }
        let valid = if index == 0 {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'
        };
        if !valid {
            return Err(StoreError::InvalidId { byte: index });
        }
    }

    if bytes.len() > 24 {
        return Err(StoreError::InvalidId { byte: 24 });
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
            streams: BTreeMap::new(),
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
            Expected::Missing => actual.is_none(),
            Expected::Exact(version) => actual == Some(version),
        };
        if !matches {
            return Err(StoreError::Conflict { expected, actual });
        }

        if events.is_empty() {
            return Ok(existing
                .map(|stream| stream.account.clone())
                .unwrap_or_else(initial_account));
        }

        let mut account = existing
            .map(|stream| stream.account.clone())
            .unwrap_or_else(initial_account);
        let old_version = account.version;

        apply_events(&mut account, events).map_err(StoreError::Rejected)?;

        let mut history = existing
            .map(|stream| stream.history.clone())
            .unwrap_or_default();
        history.extend_from_slice(events);

        let mut snapshot = existing.and_then(|stream| stream.snapshot.clone());
        for offset in 1..=events.len() {
            let offset = u64::try_from(offset).expect("event count is representable as u64");
            let version = old_version + offset;
            if version % self.snapshot_every == 0 {
                let boundary_account = if offset == events.len() as u64 {
                    account.clone()
                } else {
                    let prefix_len = usize::try_from(offset).expect("offset fits usize");
                    let mut prefix_account = existing
                        .map(|stream| stream.account.clone())
                        .unwrap_or_else(initial_account);
                    let start = history.len() - events.len();
                    apply_events(&mut prefix_account, &history[start..start + prefix_len])
                        .expect("validated batch prefix remains valid");
                    prefix_account
                };
                snapshot = Some(Snapshot {
                    tag: snapshot_tag(&boundary_account),
                    account: boundary_account,
                });
            }
        }

        self.streams.insert(
            id.to_owned(),
            Stream {
                history,
                account: account.clone(),
                snapshot,
            },
        );

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
                .history
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    u64::try_from(index + 1).map_or(true, |version| version > after)
                })
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
                .history
                .iter()
                .enumerate()
                .filter(|(index, _)| {
                    u64::try_from(index + 1).map_or(true, |version| version > snapshot_version)
                })
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

fn validate_snapshot(snapshot: &Snapshot) -> Result<(), SnapshotError> {
    let account = &snapshot.account;
    if account.version == 0 {
        return Err(SnapshotError::InvalidVersion);
    }
    if account.status == Status::New
        || !(1..=1_000_000_000).contains(&account.limit)
        || account.balance < 0
        || account.balance > account.limit
        || (account.status == Status::Closed && (account.balance != 0 || account.version < 2))
    {
        return Err(SnapshotError::InvalidState);
    }
    if snapshot.tag != snapshot_tag(account) {
        return Err(SnapshotError::TagMismatch);
    }
    Ok(())
}

pub fn rebuild(snapshot: Option<&Snapshot>, tail: &[Event]) -> Result<Account, RestoreError> {
    let mut account = match snapshot {
        None => initial_account(),
        Some(snapshot) => {
            validate_snapshot(snapshot).map_err(RestoreError::Snapshot)?;
            snapshot.account.clone()
        }
    };
    apply_events(&mut account, tail).map_err(RestoreError::Tail)?;
    Ok(account)
}