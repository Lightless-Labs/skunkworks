#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status { New, Open, Closed }

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
    VersionOverflow, InvalidLimit, AlreadyOpened, NotOpened, Closed,
    InvalidAmount, LimitExceeded, InvalidFloor, InsufficientFunds,
    FloorViolation, CloseVersion { expected: u64, actual: u64 }, NonZeroBalance
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayError {
    pub index: usize,
    pub kind: ApplyError,
}

fn initial_account() -> Account {
    Account { status: Status::New, balance: 0, limit: 0, version: 0 }
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

fn replay_from(mut account: Account, events: &[Event]) -> Result<Account, ReplayError> {
    for (index, event) in events.iter().enumerate() {
        apply_event(&mut account, event)
            .map_err(|kind| ReplayError { index, kind })?;
    }
    Ok(account)
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    replay_from(initial_account(), events)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expected { Any, Missing, Exact(u64) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    InvalidId { byte: usize },
    Conflict { expected: Expected, actual: Option<u64> },
    Rejected(ReplayError),
}

fn invalid_id_byte(id: &str) -> Option<usize> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Some(0);
    }
    for (i, &b) in bytes.iter().enumerate().take(24) {
        let valid = if i == 0 {
            b.is_ascii_lowercase()
        } else {
            b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'
        };
        if !valid {
            return Some(i);
        }
    }
    if bytes.len() > 24 {
        return Some(24);
    }
    None
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    match invalid_id_byte(id) {
        Some(byte) => Err(StoreError::InvalidId { byte }),
        None => Ok(()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub account: Account,
    pub tag: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotError { ZeroInterval, InvalidVersion, InvalidState, TagMismatch }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError { Snapshot(SnapshotError), Tail(ReplayError) }

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
    match snapshot {
        None => replay(tail).map_err(RestoreError::Tail),
        Some(snapshot) => {
            validate_snapshot(snapshot).map_err(RestoreError::Snapshot)?;
            replay_from(snapshot.account.clone(), tail).map_err(RestoreError::Tail)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub position: u64,
    pub stream: String,
    pub version: u64,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreamData {
    account: Account,
    events: Vec<Event>,
    snapshot: Option<Snapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    snapshot_every: u64,
    streams: std::collections::BTreeMap<String, StreamData>,
    records: Vec<Record>,
}

impl Store {
    pub fn new(snapshot_every: u64) -> Result<Self, SnapshotError> {
        if snapshot_every == 0 {
            return Err(SnapshotError::ZeroInterval);
        }
        Ok(Self {
            snapshot_every,
            streams: std::collections::BTreeMap::new(),
            records: Vec::new(),
        })
    }

    pub fn append(&mut self, id: &str, expected: Expected, events: &[Event]) -> Result<Account, StoreError> {
        validate_id(id)?;
        let existing = self.streams.get(id);
        let actual = existing.map(|s| s.account.version);
        let matches = match expected {
            Expected::Any => true,
            Expected::Missing => actual.is_none(),
            Expected::Exact(v) => actual == Some(v),
        };
        if !matches {
            return Err(StoreError::Conflict { expected, actual });
        }

        let start_account = existing.map(|s| s.account.clone()).unwrap_or_else(initial_account);
        if events.is_empty() {
            return Ok(start_account);
        }

        let mut account = start_account;
        let mut snapshots = Vec::new();
        for (index, event) in events.iter().enumerate() {
            apply_event(&mut account, event)
                .map_err(|kind| StoreError::Rejected(ReplayError { index, kind }))?;
            if account.version > 0 && account.version % self.snapshot_every == 0 {
                snapshots.push(Snapshot {
                    account: account.clone(),
                    tag: snapshot_tag(&account),
                });
            }
        }

        let entry = self.streams.entry(id.to_owned()).or_insert_with(|| StreamData {
            account: initial_account(),
            events: Vec::new(),
            snapshot: None,
        });
        for event in events {
            entry.events.push(event.clone());
            let version = entry.events.len() as u64;
            let position = (self.records.len() as u64) + 1;
            self.records.push(Record {
                position,
                stream: id.to_owned(),
                version,
                event: event.clone(),
            });
        }
        entry.account = account.clone();
        if let Some(snapshot) = snapshots.pop() {
            entry.snapshot = Some(snapshot);
        }
        Ok(account)
    }

    pub fn state(&self, id: &str) -> Result<Option<Account>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|s| s.account.clone()))
    }

    pub fn history(&self, id: &str, after: u64) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|s| {
            s.events.iter().enumerate()
                .filter(|(i, _)| (*i as u64) + 1 > after)
                .map(|(_, e)| e.clone())
                .collect()
        }))
    }

    pub fn snapshot(&self, id: &str) -> Result<Option<Snapshot>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).and_then(|s| s.snapshot.clone()))
    }

    pub fn tail(&self, id: &str) -> Result<Option<Vec<Event>>, StoreError> {
        validate_id(id)?;
        Ok(self.streams.get(id).map(|s| {
            let start = s.snapshot.as_ref().map(|snap| snap.account.version as usize).unwrap_or(0);
            s.events.iter().skip(start).cloned().collect()
        }))
    }

    pub fn records(&self, after: u64) -> Vec<Record> {
        self.records.iter()
            .filter(|r| r.position > after)
            .cloned()
            .collect()
    }
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
    accounts: std::collections::BTreeMap<String, Account>,
}

impl Projection {
    pub fn new() -> Self {
        Self { position: 0, journal: Vec::new(), accounts: std::collections::BTreeMap::new() }
    }

    pub fn ingest(&mut self, records: &[Record]) -> Result<usize, ProjectionError> {
        let mut provisional = self.clone();
        let mut accepted = 0;
        for (index, record) in records.iter().enumerate() {
            if let Some(byte) = invalid_id_byte(&record.stream) {
                return Err(ProjectionError::InvalidId { index, byte });
            }
            if record.position == 0 {
                return Err(ProjectionError::ZeroPosition { index });
            }
            if record.position <= provisional.position {
                let prior = &provisional.journal[(record.position - 1) as usize];
                if prior != record {
                    return Err(ProjectionError::DuplicateMismatch { index, position: record.position });
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
            let current = provisional.accounts.get(&record.stream).cloned().unwrap_or_else(initial_account);
            let expected_version = current.version + 1;
            if record.version != expected_version {
                return Err(ProjectionError::StreamVersion {
                    index,
                    expected: expected_version,
                    got: record.version,
                });
            }
            let mut next = current;
            apply_event(&mut next, &record.event)
                .map_err(|kind| ProjectionError::Rejected { index, kind })?;
            provisional.accounts.insert(record.stream.clone(), next);
            provisional.journal.push(record.clone());
            provisional.position = record.position;
            accepted += 1;
        }
        *self = provisional;
        Ok(accepted)
    }

    pub fn account(&self, id: &str) -> Result<Option<Account>, StoreError> {
        validate_id(id)?;
        Ok(self.accounts.get(id).cloned())
    }

    pub fn position(&self) -> u64 {
        self.position
    }

    pub fn totals(&self) -> Totals {
        let mut totals = Totals { streams: 0, open: 0, closed: 0, balance: 0 };
        for account in self.accounts.values() {
            totals.streams += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn o(limit: i64) -> Event { Event::Open { limit } }
    fn d(amount: i64) -> Event { Event::Deposit { amount } }
    fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
    fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
    fn failure(events: &[Event], index: usize, kind: ApplyError) {
        assert_eq!(replay(events), Err(ReplayError { index, kind }));
    }
    fn fresh() -> Store { Store::new(3).unwrap() }
    fn sealed(account: Account) -> Snapshot { Snapshot { tag: snapshot_tag(&account), account } }
    fn r(position:u64, stream:&str, version:u64, event:Event) -> Record {
        Record { position, stream:stream.to_owned(), version, event }
    }

    #[test]
    fn public_replay() {
        assert_eq!(replay(&[o(90),d(20)]).unwrap().balance,20);
    }

    #[test]
    fn public_store_log_and_snapshot() {
        let mut s=Store::new(2).unwrap();s.append("main",Expected::Missing,&[o(90),d(30),w(10,0)]).unwrap();
        assert_eq!(s.records(1),vec![r(2,"main",2,d(30)),r(3,"main",3,w(10,0))]);
        assert_eq!(s.snapshot("main").unwrap().unwrap().account.version,2);assert_eq!(s.tail("main").unwrap(),Some(vec![w(10,0)]));
        assert_eq!(s.history("main",2).unwrap(),Some(vec![w(10,0)]));assert_eq!(s.state("main").unwrap().unwrap().balance,20);
    }

    #[test]
    fn public_projection() {
        let mut p=Projection::new();let es=vec![r(1,"main",1,o(80)),r(2,"main",2,d(25)),r(3,"side",1,o(40))];
        assert_eq!(p.ingest(&es),Ok(3));assert_eq!(p.ingest(&es),Ok(0));assert_eq!(p.position(),3);
        assert_eq!(p.account("main").unwrap().unwrap().balance,25);assert_eq!(p.totals(),Totals {streams:2,open:2,closed:0,balance:25});
    }

    #[test]
    fn public_restore() {
        let a=replay(&[o(70),d(20)]).unwrap();let snap=sealed(a);let tail=[w(5,0)];
        assert_eq!(rebuild(Some(&snap),&tail).unwrap().balance,15);assert_eq!(rebuild(None,&[o(12)]).unwrap().version,1);
    }
}