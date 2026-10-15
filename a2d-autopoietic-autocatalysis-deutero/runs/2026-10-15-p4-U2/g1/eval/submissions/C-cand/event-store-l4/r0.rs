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

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut account = initial_account();
    for (index, event) in events.iter().enumerate() {
        if let Err(kind) = apply_event(&mut account, event) {
            return Err(ReplayError { index, kind });
        }
    }
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
    account: Account,
    events: Vec<Event>,
    snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub position: u64,
    pub stream: String,
    pub version: u64,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    snapshot_every: u64,
    streams: BTreeMap<String, Stream>,
    records: Vec<Record>,
}

impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 {
            return Err(SnapshotError::ZeroInterval);
        }
        Ok(Self {
            snapshot_every,
            streams: BTreeMap::new(),
            records: Vec::new(),
        })
    }

    pub fn append(
        &mut self,
        id: &str,
        expected: Expected,
        events: &[Event],
    ) -> Result<Account, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;

        let existing = self.streams.get(id);
        let actual = existing.map(|stream| stream.account.version);
        let matches = match &expected {
            Expected::Any => true,
            Expected::Missing => actual.is_none(),
            Expected::Exact(version) => actual == Some(*version),
        };
        if !matches {
            return Err(StoreError::Conflict { expected, actual });
        }

        let mut account = existing
            .map(|stream| stream.account.clone())
            .unwrap_or_else(initial_account);

        if events.is_empty() {
            return Ok(account);
        }

        let mut intermediate_snapshots = Vec::new();
        for (index, event) in events.iter().enumerate() {
            if let Err(kind) = apply_event(&mut account, event) {
                return Err(StoreError::Rejected(ReplayError { index, kind }));
            }
            if account.version > 0 && account.version % self.snapshot_every == 0 {
                intermediate_snapshots.push(Snapshot {
                    tag: snapshot_tag(&account),
                    account: account.clone(),
                });
            }
        }

        let stream = self.streams.entry(id.to_owned()).or_insert_with(|| Stream {
            account: initial_account(),
            events: Vec::new(),
            snapshot: None,
        });

        for event in events {
            stream.events.push(event.clone());
            let version = stream.account.version + 1;
            stream.account = account_for_committed_prefix(&stream.events, version);
            let position = self.records.len() as u64 + 1;
            self.records.push(Record {
                position,
                stream: id.to_owned(),
                version,
                event: event.clone(),
            });
        }

        if let Some(snapshot) = intermediate_snapshots.pop() {
            stream.snapshot = Some(snapshot);
        }

        Ok(account)
    }

    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.streams.get(id).map(|stream| stream.account.clone()))
    }

    pub fn history(&self, id: &str, after: u64) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.streams.get(id).map(|stream| {
            stream
                .events
                .iter()
                .enumerate()
                .filter(|(index, _)| (*index as u64) + 1 > after)
                .map(|(_, event)| event.clone())
                .collect()
        }))
    }

    pub fn snapshot(&self, id: &str) -> Result<Option<Snapshot>, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self
            .streams
            .get(id)
            .and_then(|stream| stream.snapshot.clone()))
    }

    pub fn tail(&self, id: &str) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
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
                .filter(|(index, _)| (*index as u64) + 1 > snapshot_version)
                .map(|(_, event)| event.clone())
                .collect()
        }))
    }

    pub fn records(&self, after: u64) -> Vec<Record> {
        self.records
            .iter()
            .filter(|record| record.position > after)
            .cloned()
            .collect()
    }
}

fn account_for_committed_prefix(events: &[Event], version: u64) -> Account {
    let mut account = initial_account();
    for event in events.iter().take(version as usize) {
        let _ = apply_event(&mut account, event);
    }
    account
}

fn validate_id(id: &str) -> Result<(), usize> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Err(0);
    }
    for (index, byte) in bytes.iter().take(24).enumerate() {
        let valid = if index == 0 {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'
        };
        if !valid {
            return Err(index);
        }
    }
    if bytes.len() > 24 {
        return Err(24);
    }
    Ok(())
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

pub fn rebuild(snapshot: Option<&Snapshot>, tail: &[Event]) -> Result<Account, RestoreError> {
    let mut account = if let Some(snapshot) = snapshot {
        validate_snapshot(snapshot).map_err(RestoreError::Snapshot)?;
        snapshot.account.clone()
    } else {
        initial_account()
    };

    for (index, event) in tail.iter().enumerate() {
        if let Err(kind) = apply_event(&mut account, event) {
            return Err(RestoreError::Tail(ReplayError { index, kind }));
        }
    }
    Ok(account)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    pub streams: u64,
    pub open: u64,
    pub closed: u64,
    pub balance: i128,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    InvalidId { index: usize, byte: usize },
    ZeroPosition { index: usize },
    DuplicateMismatch { index: usize, position: u64 },
    Gap { index: usize, expected: u64, got: u64 },
    StreamVersion { index: usize, expected: u64, got: u64 },
    Rejected { index: usize, kind: ApplyError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    position: u64,
    journal: Vec<Record>,
    accounts: BTreeMap<String, Account>,
}

impl Projection {
    pub fn new() -> Self {
        Self {
            position: 0,
            journal: Vec::new(),
            accounts: BTreeMap::new(),
        }
    }

    pub fn ingest(&mut self, records: &[Record]) -> Result<usize, ProjectionError> {
        let mut provisional = self.clone();
        let mut accepted = 0;

        for (index, record) in records.iter().enumerate() {
            if let Err(byte) = validate_id(&record.stream) {
                return Err(ProjectionError::InvalidId { index, byte });
            }
            if record.position == 0 {
                return Err(ProjectionError::ZeroPosition { index });
            }

            if record.position <= provisional.position {
                let stored = &provisional.journal[(record.position - 1) as usize];
                if stored != record {
                    return Err(ProjectionError::DuplicateMismatch {
                        index,
                        position: record.position,
                    });
                }
                continue;
            }

            let expected_position = provisional.position + 1;
            if record.position != expected_position {
                return Err(ProjectionError::Gap {
                    index,
                    expected: expected_position,
                    got: record.position,
                });
            }

            let existing = provisional.accounts.get(&record.stream);
            let expected_version = existing.map(|account| account.version + 1).unwrap_or(1);
            if record.version != expected_version {
                return Err(ProjectionError::StreamVersion {
                    index,
                    expected: expected_version,
                    got: record.version,
                });
            }

            let mut account = existing.cloned().unwrap_or_else(initial_account);
            if let Err(kind) = apply_event(&mut account, &record.event) {
                return Err(ProjectionError::Rejected { index, kind });
            }

            provisional.accounts.insert(record.stream.clone(), account);
            provisional.journal.push(record.clone());
            provisional.position = record.position;
            accepted += 1;
        }

        *self = provisional;
        Ok(accepted)
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>, StoreError> {
        validate_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.accounts.get(id).cloned())
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn totals(&self) -> Totals {
        let mut totals = Totals {
            streams: self.accounts.len() as u64,
            open: 0,
            closed: 0,
            balance: 0,
        };
        for account in self.accounts.values() {
            match account.status {
                Status::New => {}
                Status::Open => totals.open += 1,
                Status::Closed => totals.closed += 1,
            }
            totals.balance += account.balance as i128;
        }
        totals
    }
}

fn main() {}