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
    run(&start, tail).map_err(|mut e| { e.index += start.version as usize; RestoreError::Tail(e) })
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    streams: std::collections::BTreeMap<String, Vec<Event>>,
    interval: u64,
    snapshots: std::collections::BTreeMap<String, Snapshot>,
}
impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 { return Err(SnapshotError::ZeroInterval); }
        Ok(Self { streams: std::collections::BTreeMap::new(), interval: snapshot_every,
            snapshots: std::collections::BTreeMap::new(), })
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
}
