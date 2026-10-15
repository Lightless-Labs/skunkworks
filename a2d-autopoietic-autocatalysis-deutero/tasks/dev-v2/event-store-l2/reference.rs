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
pub struct Store {
    streams: std::collections::BTreeMap<String, Vec<Event>>,
}
impl Store {
    pub fn new() -> Self { Self { streams: std::collections::BTreeMap::new() } }
    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        valid_id(id)?;
        Ok(self.streams.get(id).map(|events| replay(events).expect("internal valid history")))
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
        self.streams.entry(id.to_owned()).or_default().extend_from_slice(events);
        Ok(result)
    }
}
