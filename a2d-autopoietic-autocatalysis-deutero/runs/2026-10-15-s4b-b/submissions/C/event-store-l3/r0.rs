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
    let next_version: u64 = account
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
            if !(0..=account.limit).contains(floor) {
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

fn apply_events(start: &Account, events: &[Event]) -> Result<Account, ReplayError> {
    let mut account: Account = start.clone();
    for (index, event) in events.iter().enumerate() {
        if let Err(kind) = apply_event(&mut account, event) {
            return Err(ReplayError { index, kind });
        }
    }
    Ok(account)
}

pub fn replay(events: &[Event]) -> Result<Account, ReplayError> {
    apply_events(&initial_account(), events)
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
    streams: std::collections::BTreeMap<String, Stream>,
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    let bytes: &[u8] = id.as_bytes();
    if bytes.is_empty() {
        return Err(StoreError::InvalidId { byte: 0 });
    }
    for (index, &byte) in bytes.iter().enumerate() {
        let valid: bool = if index == 0 {
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

        let existing: Option<&Stream> = self.streams.get(id);
        let actual: Option<u64> = existing.map(|stream| stream.account.version);
        let matches: bool = match &expected {
            Expected::Any => true,
            Expected::Missing => actual.is_none(),
            Expected::Exact(version) => actual == Some(*version),
        };
        if !matches {
            return Err(StoreError::Conflict { expected, actual });
        }

        let start: Account = existing
            .map(|stream| stream.account.clone())
            .unwrap_or_else(initial_account);
        if events.is_empty() {
            return Ok(start);
        }

        let final_account: Account =
            apply_events(&start, events).map_err(StoreError::Rejected)?;

        let mut history: Vec<Event> = existing
            .map(|stream| stream.history.clone())
            .unwrap_or_default();
        history.extend_from_slice(events);

        let mut snapshot: Option<Snapshot> =
            existing.and_then(|stream| stream.snapshot.clone());
        let mut running: Account = start;
        for event in events {
            apply_event(&mut running, event).expect("batch was validated");
            if running.version > 0 && running.version % self.snapshot_every == 0 {
                snapshot = Some(Snapshot {
                    tag: snapshot_tag(&running),
                    account: running.clone(),
                });
            }
        }

        let result: Account = final_account.clone();
        self.streams.insert(
            id.to_string(),
            Stream {
                history,
                account: final_account,
                snapshot,
            },
        );
        Ok(result)
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
            let start: usize = stream
                .snapshot
                .as_ref()
                .map(|snapshot| snapshot.account.version as usize)
                .unwrap_or(0)
                .min(stream.history.len());
            stream.history[start..].to_vec()
        }))
    }
}

pub fn snapshot_tag(account: &Account) -> u64 {
    let status_code: u64 = match account.status {
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
    if snapshot.account.version == 0 {
        return Err(SnapshotError::InvalidVersion);
    }
    let account: &Account = &snapshot.account;
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

pub fn rebuild(snapshot: Option<&Snapshot>, tail: &[Event]) -> Result<Account, RestoreError> {
    match snapshot {
        None => replay(tail).map_err(RestoreError::Tail),
        Some(snapshot) => {
            validate_snapshot(snapshot).map_err(RestoreError::Snapshot)?;
            apply_events(&snapshot.account, tail).map_err(RestoreError::Tail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(limit: i64) -> Event {
        Event::Open { limit }
    }

    fn d(amount: i64) -> Event {
        Event::Deposit { amount }
    }

    fn w(amount: i64, floor: i64) -> Event {
        Event::Withdraw { amount, floor }
    }

    fn c(expected_version: u64) -> Event {
        Event::Close { expected_version }
    }

    fn fresh() -> Store {
        Store::new(3).unwrap()
    }

    #[test]
    fn replay_and_error_precedence() {
        assert_eq!(replay(&[]).unwrap(), initial_account());
        assert_eq!(replay(&[o(90), d(20)]).unwrap().balance, 20);
        assert_eq!(
            replay(&[o(90), d(20), w(10, 11)]),
            Err(ReplayError {
                index: 2,
                kind: ApplyError::FloorViolation
            })
        );
        assert_eq!(
            replay(&[o(90), o(0)]),
            Err(ReplayError {
                index: 1,
                kind: ApplyError::InvalidLimit
            })
        );
        assert_eq!(
            replay(&[o(90), d(i64::MAX)]),
            Err(ReplayError {
                index: 1,
                kind: ApplyError::LimitExceeded
            })
        );
    }

    #[test]
    fn store_append_reads_and_checkpoints() {
        let mut store: Store = Store::new(2).unwrap();
        let events: Vec<Event> = vec![o(90), d(30), w(10, 0)];
        store
            .append("main", Expected::Missing, &events)
            .unwrap();
        let snapshot: Snapshot = store.snapshot("main").unwrap().unwrap();
        assert_eq!(snapshot.account, replay(&events[..2]).unwrap());
        assert_eq!(store.tail("main").unwrap(), Some(vec![w(10, 0)]));
        assert_eq!(store.state("main").unwrap().unwrap(), replay(&events).unwrap());
        assert_eq!(store.history("main", 2).unwrap(), Some(vec![w(10, 0)]));
    }

    #[test]
    fn append_is_atomic_and_empty_append_does_not_create() {
        let mut store: Store = fresh();
        assert_eq!(
            store.append("main", Expected::Missing, &[]).unwrap(),
            initial_account()
        );
        assert_eq!(store.state("main").unwrap(), None);

        store
            .append("main", Expected::Missing, &[o(20), d(5)])
            .unwrap();
        let before: Store = store.clone();
        assert!(store
            .append("main", Expected::Exact(2), &[d(1), w(100, 0)])
            .is_err());
        assert_eq!(store, before);
    }

    #[test]
    fn close_and_snapshot_validation() {
        let events: Vec<Event> = vec![o(10), d(5), w(5, 0), c(3)];
        let account: Account = replay(&events).unwrap();
        assert_eq!(account.status, Status::Closed);
        assert_eq!(account.version, 4);

        let valid: Snapshot = Snapshot {
            tag: snapshot_tag(&account),
            account: account.clone(),
        };
        assert_eq!(rebuild(Some(&valid), &[]).unwrap(), account);
        let bad_tag: Snapshot = Snapshot {
            tag: valid.tag ^ 1,
            account: account.clone(),
        };
        assert_eq!(
            rebuild(Some(&bad_tag), &[d(1)]),
            Err(RestoreError::Snapshot(SnapshotError::TagMismatch))
        );
    }

    #[test]
    fn invalid_id_reports_first_bad_byte() {
        let mut store: Store = fresh();
        assert_eq!(
            store.state("").unwrap_err(),
            StoreError::InvalidId { byte: 0 }
        );
        assert_eq!(
            store.state("abcé").unwrap_err(),
            StoreError::InvalidId { byte: 3 }
        );
        assert_eq!(
            store.state("abcdefghijklmnopqrstuvwxy").unwrap_err(),
            StoreError::InvalidId { byte: 24 }
        );
    }
}

fn main() {}