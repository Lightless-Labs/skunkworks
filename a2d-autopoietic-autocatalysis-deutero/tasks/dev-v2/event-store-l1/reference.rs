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
