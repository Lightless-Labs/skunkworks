use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status { New, Open, Closed }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account { pub status: Status, pub balance: i64, pub limit: i64, pub version: u64 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Open { limit: i64 }, Deposit { amount: i64 },
    Withdraw { amount: i64, floor: i64 }, Close { expected_version: u64 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyError {
    VersionOverflow, InvalidLimit, AlreadyOpened, NotOpened, Closed,
    InvalidAmount, LimitExceeded, InvalidFloor, InsufficientFunds,
    FloorViolation, CloseVersion { expected: u64, actual: u64 }, NonZeroBalance
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError { pub index: usize, pub kind: ApplyError }

fn initial_account() -> Account {
    Account { status: Status::New, balance: 0, limit: 0, version: 0 }
}

fn apply_event(a: &mut Account, e: &Event) -> Result<(), ApplyError> {
    if a.version == u64::MAX { return Err(ApplyError::VersionOverflow); }

    match e {
        Event::Open { limit } => {
            if *limit < 1 || *limit > 1_000_000_000 { return Err(ApplyError::InvalidLimit); }
            if a.status != Status::New { return Err(ApplyError::AlreadyOpened); }
            a.status = Status::Open;
            a.limit = *limit;
        }
        Event::Deposit { amount } => {
            match a.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
            }
            if *amount <= 0 { return Err(ApplyError::InvalidAmount); }
            if *amount > a.limit - a.balance { return Err(ApplyError::LimitExceeded); }
            a.balance += *amount;
        }
        Event::Withdraw { amount, floor } => {
            match a.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
            }
            if *amount <= 0 { return Err(ApplyError::InvalidAmount); }
            if *floor < 0 || *floor > a.limit { return Err(ApplyError::InvalidFloor); }
            if *amount > a.balance { return Err(ApplyError::InsufficientFunds); }
            if a.balance - *amount < *floor { return Err(ApplyError::FloorViolation); }
            a.balance -= *amount;
        }
        Event::Close { expected_version } => {
            match a.status {
                Status::New => return Err(ApplyError::NotOpened),
                Status::Closed => return Err(ApplyError::Closed),
                Status::Open => {}
            }
            if *expected_version != a.version {
                return Err(ApplyError::CloseVersion { expected: *expected_version, actual: a.version });
            }
            if a.balance != 0 { return Err(ApplyError::NonZeroBalance); }
            a.status = Status::Closed;
        }
    }
    a.version += 1;
    Ok(())
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    let mut a = initial_account();
    for (index, event) in events.iter().enumerate() {
        if let Err(kind) = apply_event(&mut a, event) {
            return Err(ReplayError { index, kind });
        }
    }
    Ok(a)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected { Any, Missing, Exact(u64) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidId { byte: usize },
    Conflict { expected: Expected, actual: Option<u64> },
    Rejected(ReplayError)
}

fn valid_id(id: &str) -> Result<(), usize> {
    let b = id.as_bytes();
    if b.is_empty() { return Err(0); }
    for (i, x) in b.iter().enumerate() {
        if i == 24 { return Err(24); }
        let ok = if i == 0 { x.is_ascii_lowercase() }
            else { x.is_ascii_lowercase() || x.is_ascii_digit() || *x == b'-' };
        if !ok { return Err(i); }
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot { pub account: Account, pub tag: u64 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError { ZeroInterval, InvalidVersion, InvalidState, TagMismatch }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError { Snapshot(SnapshotError), Tail(ReplayError) }

pub fn snapshot_tag(a: &Account) -> u64 {
    let code = match a.status { Status::New => 0, Status::Open => 0xA5, Status::Closed => 0x5A };
    a.version.rotate_left(7)
        ^ (a.limit as u64).wrapping_mul(31)
        ^ (a.balance as u64).rotate_left(17)
        ^ code
}

fn check_snapshot(s: &Snapshot) -> Result<(), SnapshotError> {
    let a = &s.account;
    if a.version == 0 { return Err(SnapshotError::InvalidVersion); }
    if a.status == Status::New || a.limit < 1 || a.limit > 1_000_000_000
        || a.balance < 0 || a.balance > a.limit
        || (a.status == Status::Closed && (a.balance != 0 || a.version < 2))
    {
        return Err(SnapshotError::InvalidState);
    }
    if s.tag != snapshot_tag(a) { return Err(SnapshotError::TagMismatch); }
    Ok(())
}

pub fn rebuild(snapshot: Option<&Snapshot>, tail: &[Event]) -> Result<Account, RestoreError> {
    let mut a = match snapshot {
        Some(s) => {
            check_snapshot(s).map_err(RestoreError::Snapshot)?;
            s.account.clone()
        }
        None => initial_account(),
    };
    for (index, event) in tail.iter().enumerate() {
        if let Err(kind) = apply_event(&mut a, event) {
            return Err(RestoreError::Tail(ReplayError { index, kind }));
        }
    }
    Ok(a)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record { pub position: u64, pub stream: String, pub version: u64, pub event: Event }

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamData {
    account: Account,
    events: Vec<Event>,
    snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    interval: u64,
    data: BTreeMap<String, StreamData>,
    log: Vec<Record>,
}

impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 { return Err(SnapshotError::ZeroInterval); }
        Ok(Self { interval: snapshot_every, data: BTreeMap::new(), log: Vec::new() })
    }

    pub fn append(&mut self, id: &str, expected: Expected, events: &[Event]) -> Result<Account, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        let old = self.data.get(id);
        let actual = old.map(|x| x.account.version);
        let matched = match &expected {
            Expected::Any => true,
            Expected::Missing => actual.is_none(),
            Expected::Exact(v) => actual == Some(*v),
        };
        if !matched { return Err(StoreError::Conflict { expected, actual }); }

        let mut new_account = old.map(|x| x.account.clone()).unwrap_or_else(initial_account);
        if events.is_empty() { return Ok(new_account); }

        let base = new_account.version;
        let mut new_events = Vec::with_capacity(events.len());
        let mut new_snap = old.and_then(|x| x.snapshot.clone());
        for (index, event) in events.iter().enumerate() {
            if let Err(kind) = apply_event(&mut new_account, event) {
                return Err(StoreError::Rejected(ReplayError { index, kind }));
            }
            new_events.push(event.clone());
            if new_account.version % self.interval == 0 {
                new_snap = Some(Snapshot {
                    account: new_account.clone(),
                    tag: snapshot_tag(&new_account),
                });
            }
        }

        let global_base = self.log.len() as u64;
        let additions: Vec<Record> = new_events.iter().enumerate().map(|(i, event)| Record {
            position: global_base + i as u64 + 1,
            stream: id.to_owned(),
            version: base + i as u64 + 1,
            event: event.clone(),
        }).collect();

        let entry = self.data.entry(id.to_owned()).or_insert_with(|| StreamData {
            account: initial_account(),
            events: Vec::new(),
            snapshot: None,
        });
        entry.account = new_account.clone();
        entry.events.extend(new_events);
        entry.snapshot = new_snap;
        self.log.extend(additions);
        Ok(new_account)
    }

    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.data.get(id).map(|x| x.account.clone()))
    }

    pub fn history(&self, id: &str, after: u64) -> Result<Option<Vec<Event>>, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.data.get(id).map(|x| x.events.iter().enumerate()
            .filter(|(i, _)| (*i as u64) + 1 > after)
            .map(|(_, e)| e.clone()).collect()))
    }

    pub fn snapshot(&self, id: &str) -> Result<Option<Snapshot>, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.data.get(id).and_then(|x| x.snapshot.clone()))
    }

    pub fn tail(&self, id: &str) -> Result<Option<Vec<Event>>, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.data.get(id).map(|x| {
            let version = x.snapshot.as_ref().map(|s| s.account.version).unwrap_or(0);
            let start = usize::try_from(version).unwrap_or(usize::MAX).min(x.events.len());
            x.events[start..].to_vec()
        }))
    }

    pub fn records(&self, after: u64) -> Vec<Record> {
        self.log.iter().filter(|r| r.position > after).cloned().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals { pub streams: u64, pub open: u64, pub closed: u64, pub balance: i128 }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    InvalidId { index: usize, byte: usize }, ZeroPosition { index: usize },
    DuplicateMismatch { index: usize, position: u64 },
    Gap { index: usize, expected: u64, got: u64 },
    StreamVersion { index: usize, expected: u64, got: u64 },
    Rejected { index: usize, kind: ApplyError }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Projection {
    cursor: u64,
    journal: Vec<Record>,
    accounts: BTreeMap<String, Account>,
}

impl Projection {
    pub fn new() -> Self {
        Self { cursor: 0, journal: Vec::new(), accounts: BTreeMap::new() }
    }

    pub fn ingest(&mut self, records: &[Record]) -> Result<usize, ProjectionError> {
        let mut work = self.clone();
        let mut added = 0usize;
        for (index, record) in records.iter().enumerate() {
            if let Err(byte) = valid_id(&record.stream) {
                return Err(ProjectionError::InvalidId { index, byte });
            }
            if record.position == 0 {
                return Err(ProjectionError::ZeroPosition { index });
            }
            if record.position <= work.cursor {
                let previous = &work.journal[(record.position - 1) as usize];
                if previous != record {
                    return Err(ProjectionError::DuplicateMismatch { index, position: record.position });
                }
                continue;
            }
            let expected_position = work.cursor + 1;
            if record.position != expected_position {
                return Err(ProjectionError::Gap { index, expected: expected_position, got: record.position });
            }

            let current_version = work.accounts.get(&record.stream).map(|a| a.version).unwrap_or(0);
            let expected_version = current_version + 1;
            if record.version != expected_version {
                return Err(ProjectionError::StreamVersion { index, expected: expected_version, got: record.version });
            }
            let mut account = work.accounts.get(&record.stream).cloned().unwrap_or_else(initial_account);
            if let Err(kind) = apply_event(&mut account, &record.event) {
                return Err(ProjectionError::Rejected { index, kind });
            }
            work.accounts.insert(record.stream.clone(), account);
            work.journal.push(record.clone());
            work.cursor = record.position;
            added += 1;
        }
        *self = work;
        Ok(added)
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>, StoreError> {
        valid_id(id).map_err(|byte| StoreError::InvalidId { byte })?;
        Ok(self.accounts.get(id).cloned())
    }

    pub fn position(&self) -> u64 { self.cursor }

    pub fn totals(&self) -> Totals {
        let mut t = Totals { streams: self.accounts.len() as u64, open: 0, closed: 0, balance: 0 };
        for a in self.accounts.values() {
            match a.status { Status::Open => t.open += 1, Status::Closed => t.closed += 1, Status::New => {} }
            t.balance += a.balance as i128;
        }
        t
    }
}
