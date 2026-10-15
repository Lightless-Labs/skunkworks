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

    account.version = account
        .version
        .checked_add(1)
        .ok_or(ApplyError::VersionOverflow)?;
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
struct Stream {
    account: Account,
    events: Vec<Event>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Store {
    streams: std::collections::BTreeMap<String, Stream>,
}

fn validate_id(id: &str) -> Result<(), StoreError> {
    let bytes = id.as_bytes();
    if bytes.is_empty() {
        return Err(StoreError::InvalidId { byte: 0 });
    }
    for (index, byte) in bytes.iter().enumerate() {
        let valid = if index == 0 {
            byte.is_ascii_lowercase()
        } else {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-'
        };
        if !valid {
            return Err(StoreError::InvalidId { byte: index });
        }
        if index == 24 {
            return Err(StoreError::InvalidId { byte: 24 });
        }
    }
    Ok(())
}

impl Store {
    pub fn new() -> Self {
        Self {
            streams: std::collections::BTreeMap::new(),
        }
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

        for (index, event) in events.iter().enumerate() {
            if let Err(kind) = apply_event(&mut account, event) {
                return Err(StoreError::Rejected(ReplayError { index, kind }));
            }
        }

        if events.is_empty() {
            return Ok(account);
        }

        let stream = self.streams.entry(id.to_owned()).or_insert_with(|| Stream {
            account: initial_account(),
            events: Vec::new(),
        });
        stream.events.extend_from_slice(events);
        stream.account = account.clone();
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

    #[test]
    fn replay_and_empty_initial_state() {
        assert_eq!(
            replay(&[]).unwrap(),
            Account {
                status: Status::New,
                balance: 0,
                limit: 0,
                version: 0,
            }
        );
        assert_eq!(replay(&[o(90), d(20)]).unwrap().balance, 20);
    }

    #[test]
    fn error_precedence_and_boundaries() {
        assert_eq!(
            replay(&[o(0), o(10)]).unwrap_err().kind,
            ApplyError::InvalidLimit
        );
        assert_eq!(
            replay(&[o(10), o(20)]).unwrap_err().kind,
            ApplyError::AlreadyOpened
        );
        assert_eq!(
            replay(&[d(0)]).unwrap_err().kind,
            ApplyError::NotOpened
        );
        assert_eq!(
            replay(&[o(10), d(0)]).unwrap_err().kind,
            ApplyError::InvalidAmount
        );
        assert_eq!(
            replay(&[o(10), d(i64::MAX)]).unwrap_err().kind,
            ApplyError::LimitExceeded
        );
        assert_eq!(
            replay(&[o(10), w(i64::MAX, 11)]).unwrap_err().kind,
            ApplyError::InvalidFloor
        );
        assert_eq!(
            replay(&[o(10), w(i64::MAX, 0)]).unwrap_err().kind,
            ApplyError::InsufficientFunds
        );
    }

    #[test]
    fn closing_and_versioning() {
        let account = replay(&[o(10), c(1)]).unwrap();
        assert_eq!(account.status, Status::Closed);
        assert_eq!(account.version, 2);
        assert_eq!(
            replay(&[o(10), d(1), c(1)]).unwrap_err().kind,
            ApplyError::CloseVersion {
                expected: 1,
                actual: 2
            }
        );
        assert_eq!(
            replay(&[o(10), d(1), c(2)]).unwrap_err().kind,
            ApplyError::NonZeroBalance
        );
    }

    #[test]
    fn store_append_conflict_rejection_and_history() {
        let mut store = Store::new();
        store.append("main", Expected::Missing, &[o(90), d(30)]).unwrap();
        assert_eq!(
            store.append("main", Expected::Exact(2), &[w(10, 0)]).unwrap().balance,
            20
        );
        assert_eq!(store.state("main").unwrap().unwrap().version, 3);
        assert_eq!(store.history("main", 2).unwrap(), Some(vec![w(10, 0)]));
        let before = store.clone();
        assert_eq!(
            store.append("main", Expected::Any, &[d(0)]),
            Err(StoreError::Rejected(ReplayError {
                index: 0,
                kind: ApplyError::InvalidAmount
            }))
        );
        assert_eq!(store, before);
        assert_eq!(
            store.append("main", Expected::Exact(2), &[]),
            Err(StoreError::Conflict {
                expected: Expected::Exact(2),
                actual: Some(3)
            })
        );
    }

    #[test]
    fn ids_and_empty_append() {
        let mut store = Store::new();
        assert_eq!(
            store.state("").unwrap_err(),
            StoreError::InvalidId { byte: 0 }
        );
        assert_eq!(
            store.state("aA").unwrap_err(),
            StoreError::InvalidId { byte: 1 }
        );
        assert_eq!(
            store.state("abcdefghijklmnopqrstuvwxy").unwrap_err(),
            StoreError::InvalidId { byte: 24 }
        );
        assert_eq!(store.append("a", Expected::Missing, &[]).unwrap().version, 0);
        assert_eq!(store.state("a").unwrap(), None);
        assert_eq!(store.history("a", u64::MAX).unwrap(), None);
    }
}

fn main() {}