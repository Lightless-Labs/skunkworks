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
fn initial() -> Account {
    Account { status: Status::New, balance: 0, limit: 0, version: 0 }
}
fn active(a: &Account) -> Result<(), ApplyError> {
    match a.status {
        Status::New => Err(ApplyError::NotOpened),
        Status::Closed => Err(ApplyError::Closed),
        Status::Open => Ok(())
    }
}
fn step(a: &Account, e: &Event) -> Result<Account, ApplyError> {
    if a.version == u64::MAX { return Err(ApplyError::VersionOverflow); }
    let mut b = a.clone();
    match *e {
        Event::Open { limit } => {
            if !(1..=1_000_000_000).contains(&limit) { return Err(ApplyError::InvalidLimit); }
            if a.status != Status::New { return Err(ApplyError::AlreadyOpened); }
            b.status = Status::Open;
            b.limit = limit;
        }
        Event::Deposit { amount } => {
            active(a)?;
            if amount <= 0 { return Err(ApplyError::InvalidAmount); }
            if amount > a.limit - a.balance { return Err(ApplyError::LimitExceeded); }
            b.balance += amount;
        }
        Event::Withdraw { amount, floor } => {
            active(a)?;
            if amount <= 0 { return Err(ApplyError::InvalidAmount); }
            if !(0..=a.limit).contains(&floor) { return Err(ApplyError::InvalidFloor); }
            if amount > a.balance { return Err(ApplyError::InsufficientFunds); }
            if a.balance - amount < floor { return Err(ApplyError::FloorViolation); }
            b.balance -= amount;
        }
        Event::Close { expected_version } => {
            active(a)?;
            if expected_version != a.version {
                return Err(ApplyError::CloseVersion { expected: expected_version, actual: a.version });
            }
            if a.balance != 0 { return Err(ApplyError::NonZeroBalance); }
            b.status = Status::Closed;
        }
    }
    b.version += 1;
    Ok(b)
}
fn run(start: &Account, events: &[Event]) -> Result<Account, ReplayError> {
    let mut a = start.clone();
    for (index, event) in events.iter().enumerate() {
        a = step(&a, event).map_err(|kind| ReplayError { index, kind })?;
    }
    Ok(a)
}
pub fn replay(events: &[Event]) -> Result<Account, ReplayError> { run(&initial(), events) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected { Any, Missing, Exact(u64) }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidId { byte: usize },
    Conflict { expected: Expected, actual: Option<u64> },
    Rejected(ReplayError)
}
fn invalid_id(id: &str) -> Option<usize> {
    if id.is_empty() { return Some(0); }
    for (i, b) in id.bytes().enumerate() {
        if i >= 24 || !(b.is_ascii_lowercase() || (i > 0 && (b.is_ascii_digit() || b == b'-'))) {
            return Some(i);
        }
    }
    None
}
fn valid_id(id: &str) -> Result<(), StoreError> {
    match invalid_id(id) { Some(byte) => Err(StoreError::InvalidId { byte }), None => Ok(()) }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot { pub account: Account, pub tag: u64 }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError { ZeroInterval, InvalidVersion, InvalidState, TagMismatch }
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError { Snapshot(SnapshotError), Tail(ReplayError) }
pub fn snapshot_tag(a: &Account) -> u64 {
    let code = match a.status { Status::New => 0, Status::Open => 0xA5, Status::Closed => 0x5A };
    a.version.rotate_left(7) ^ (a.limit as u64).wrapping_mul(31)
        ^ (a.balance as u64).rotate_left(17) ^ code
}
fn check_snapshot(s: &Snapshot) -> Result<(), SnapshotError> {
    let a = &s.account;
    if a.version == 0 { return Err(SnapshotError::InvalidVersion); }
    if a.status == Status::New || !(1..=1_000_000_000).contains(&a.limit)
        || a.balance < 0 || a.balance > a.limit
        || (a.status == Status::Closed && (a.balance != 0 || a.version < 2)) {
        return Err(SnapshotError::InvalidState);
    }
    if snapshot_tag(a) != s.tag { return Err(SnapshotError::TagMismatch); }
    Ok(())
}
pub fn rebuild(snapshot: Option<&Snapshot>, tail: &[Event]) -> Result<Account, RestoreError> {
    let start = match snapshot {
        Some(s) => { check_snapshot(s).map_err(RestoreError::Snapshot)?; s.account.clone() }
        None => initial()
    };
    run(&start, tail).map_err(RestoreError::Tail)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    streams: std::collections::BTreeMap<String, Vec<Event>>,
    interval: u64,
    snapshots: std::collections::BTreeMap<String, Snapshot>,
    log: Vec<Record>,
}
impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 { return Err(SnapshotError::ZeroInterval); }
        Ok(Self { streams: std::collections::BTreeMap::new(), interval: snapshot_every,
            snapshots: std::collections::BTreeMap::new(), log: Vec::new(), })
    }
    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        valid_id(id)?;
        match self.streams.get(id) {
            None => Ok(None),
            Some(events) => {
                let checkpoint = self.snapshots.get(id);
                let offset = checkpoint.map(|s| s.account.version as usize).unwrap_or(0);
                Ok(Some(rebuild(checkpoint, &events[offset..]).expect("internal valid history")))
            }
        }
    }
    pub fn history(&self, id: &str, after: u64) -> Result<Option<Vec<Event>>, StoreError> {
        valid_id(id)?;
        Ok(self.streams.get(id).map(|events| events.iter().enumerate()
            .filter(|(i, _)| (*i as u64) >= after).map(|(_, e)| e.clone()).collect()))
    }
    pub fn append(&mut self, id: &str, expected: Expected, events: &[Event]) -> Result<Account, StoreError> {
        valid_id(id)?;
        let current = self.state(id)?;
        let actual = current.as_ref().map(|a| a.version);
        let matches = match &expected {
            Expected::Any => true,
            Expected::Missing => actual.is_none(),
            Expected::Exact(v) => actual == Some(*v),
        };
        if !matches { return Err(StoreError::Conflict { expected, actual }); }
        let start = current.unwrap_or_else(initial);
        let result = run(&start, events).map_err(StoreError::Rejected)?;
        if events.is_empty() { return Ok(result); }
        let mut a = start.clone();
        let mut checkpoint = self.snapshots.get(id).cloned();
        for event in events {
            a = step(&a, event).expect("validated batch");
            if a.version % self.interval == 0 {
                checkpoint = Some(Snapshot { tag: snapshot_tag(&a), account: a.clone() });
            }
        }
        if let Some(s) = checkpoint { self.snapshots.insert(id.to_owned(), s); }
        for (i, event) in events.iter().enumerate() {
            self.log.push(Record { position: self.log.len() as u64 + 1,
                stream: id.to_owned(), version: start.version + i as u64 + 1, event: event.clone() });
        }
        self.streams.entry(id.to_owned()).or_default().extend_from_slice(events);
        Ok(result)
    }
    pub fn snapshot(&self, id: &str) -> Result<Option<Snapshot>, StoreError> {
        valid_id(id)?;
        Ok(self.snapshots.get(id).cloned())
    }
    pub fn tail(&self, id: &str) -> Result<Option<Vec<Event>>, StoreError> {
        valid_id(id)?;
        let after = self.snapshots.get(id).map(|s| s.account.version).unwrap_or(0);
        self.history(id, after)
    }
    pub fn records(&self, after: u64) -> Vec<Record> {
        self.log.iter().filter(|r| r.position > after).cloned().collect()
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record { pub position: u64, pub stream: String, pub version: u64, pub event: Event }
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
    accounts: std::collections::BTreeMap<String, Account>,
    journal: std::collections::BTreeMap<u64, Record>,
    cursor: u64,
}
impl Projection {
    pub fn new() -> Self { Self { accounts: std::collections::BTreeMap::new(), journal: std::collections::BTreeMap::new(), cursor: 0 } }
    pub fn ingest(&mut self, records: &[Record]) -> Result<usize, ProjectionError> {
        let mut next = self.clone();
        let mut count = 0;
        for (index, record) in records.iter().enumerate() {
            if let Some(byte) = invalid_id(&record.stream) { return Err(ProjectionError::InvalidId { index, byte }); }
            if record.position == 0 { return Err(ProjectionError::ZeroPosition { index }); }
            if record.position <= next.cursor {
                if next.journal.get(&record.position).map(|old| (&old.stream, old.version)) != Some((&record.stream, record.version)) {
                    return Err(ProjectionError::DuplicateMismatch { index, position: record.position });
                }
                continue;
            }
            if record.position != next.cursor + 1 {
                return Err(ProjectionError::Gap { index, expected: next.cursor + 1, got: record.position });
            }
            let old = next.accounts.get(&record.stream).cloned().unwrap_or_else(initial);
            if record.version != old.version + 1 {
                return Err(ProjectionError::StreamVersion { index, expected: old.version + 1, got: record.version });
            }
            let a = step(&old, &record.event).map_err(|kind| ProjectionError::Rejected { index, kind })?;
            next.accounts.insert(record.stream.clone(), a);
            next.journal.insert(record.position, record.clone());
            next.cursor = record.position;
            count += 1;
        }
        *self = next;
        Ok(count)
    }
    pub fn account(&self, id: &str) -> Result<Option<Account>, StoreError> {
        valid_id(id)?;
        Ok(self.accounts.get(id).cloned())
    }
    pub fn position(&self) -> u64 { self.cursor }
    pub fn totals(&self) -> Totals {
        let mut t = Totals { streams: self.accounts.len() as u64, open: 0, closed: 0, balance: 0 };
        for a in self.accounts.values() {
            if a.status == Status::Open { t.open += 1; }
            if a.status == Status::Closed { t.closed += 1; }
            t.balance += a.balance as i128;
        }
        t
    }
}
