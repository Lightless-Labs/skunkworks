fn o(limit: i64) -> Event { Event::Open { limit } }
fn d(amount: i64) -> Event { Event::Deposit { amount } }
fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
fn failure(events: &[Event], index: usize, kind: ApplyError) {
    assert_eq!(replay(events), Err(ReplayError { index, kind }));
}
// R1, R2, R3, R20
#[test]
fn h_empty_and_owned() {
let a = replay(&[]).unwrap();
assert_eq!(a, Account { status: Status::New, balance: 0, limit: 0, version: 0 });
let events = vec![o(73), d(19)]; let copy = events.clone();
assert_eq!(replay(&events), replay(&events)); assert_eq!(events, copy);
}

// R5, R9, R12, R16
#[test]
fn h_unopened_precedence() {
for e in [d(0), d(i64::MAX), w(-7, -8), w(6, 0), c(888)] {
    failure(&[e, o(9)], 0, ApplyError::NotOpened);
}
}

// R4, R7, R8
#[test]
fn h_open_range() {
for n in [i64::MIN, -1, 0, 1_000_000_001, i64::MAX] { failure(&[o(n)], 0, ApplyError::InvalidLimit); }
for n in [1, 1_000_000_000] {
    assert_eq!(replay(&[o(n)]).unwrap(), Account { status: Status::Open, balance: 0, limit: n, version: 1 });
}
}

// R7, R8, R19
#[test]
fn h_open_state_priority() {
failure(&[o(11), o(0)], 1, ApplyError::InvalidLimit);
failure(&[o(11), o(8)], 1, ApplyError::AlreadyOpened);
failure(&[o(11), c(1), o(-1)], 2, ApplyError::InvalidLimit);
failure(&[o(11), c(1), o(12)], 2, ApplyError::AlreadyOpened);
}

// R9, R10, R11
#[test]
fn h_deposit_capacity() {
for amount in [i64::MIN, -1, 0] { failure(&[o(23), d(23), d(amount)], 2, ApplyError::InvalidAmount); }
failure(&[o(23), d(19), d(5)], 2, ApplyError::LimitExceeded);
failure(&[o(1_000_000_000), d(i64::MAX)], 1, ApplyError::LimitExceeded);
assert_eq!(replay(&[o(23), d(19), d(4)]).unwrap().balance, 23);
}

// R12, R13
#[test]
fn h_withdraw_fields() {
for amount in [i64::MIN, -1, 0] { failure(&[o(21), w(amount, -1)], 1, ApplyError::InvalidAmount); }
for floor in [-1, 22, i64::MIN, i64::MAX] { failure(&[o(21), w(1, floor)], 1, ApplyError::InvalidFloor); }
failure(&[o(21), w(1, 21)], 1, ApplyError::InsufficientFunds);
}

// R13, R14, R15
#[test]
fn h_withdraw_order() {
failure(&[o(44), d(12), w(13, -1)], 2, ApplyError::InvalidFloor);
failure(&[o(44), d(12), w(13, 11)], 2, ApplyError::InsufficientFunds);
failure(&[o(44), d(12), w(i64::MAX, 0)], 2, ApplyError::InsufficientFunds);
failure(&[o(44), d(12), w(5, 8)], 2, ApplyError::FloorViolation);
assert_eq!(replay(&[o(44), d(12), w(5, 7), w(7, 0)]).unwrap().balance, 0);
}

// R16, R17, R18
#[test]
fn h_close_error_order() {
failure(&[o(32), d(7), c(1)], 2, ApplyError::CloseVersion { expected: 1, actual: 2 });
failure(&[o(32), d(7), c(2)], 2, ApplyError::NonZeroBalance);
failure(&[o(32), c(0)], 1, ApplyError::CloseVersion { expected: 0, actual: 1 });
failure(&[o(32), c(u64::MAX)], 1, ApplyError::CloseVersion { expected: u64::MAX, actual: 1 });
}

// R4, R16, R19
#[test]
fn h_closed_is_terminal() {
assert_eq!(replay(&[o(91), c(1)]).unwrap(), Account { status: Status::Closed, balance: 0, limit: 91, version: 2 });
for e in [d(-1), w(0, -1), c(0), c(2)] { failure(&[o(91), c(1), e], 2, ApplyError::Closed); }
}

// R5, R20
#[test]
fn h_first_bad_index() {
failure(&[o(100), d(8), w(3, 0), d(0), o(-1)], 3, ApplyError::InvalidAmount);
failure(&[o(0), d(-1)], 0, ApplyError::InvalidLimit);
}

// R4, R6, R15, R20
#[test]
fn h_many_events_counted() {
let mut events = vec![o(1000)];
for _ in 0..300 { events.push(d(2)); events.push(w(2, 0)); }
events.push(c(601));
assert_eq!(replay(&events).unwrap(), Account { status: Status::Closed, balance: 0, limit: 1000, version: 602 });
}

// R4, R6, R11, R15, R18
#[test]
fn h_floor_is_local() {
let events = [o(100), d(80), w(30, 50), w(50, 0), d(100), w(100, 0), c(6)];
let a = replay(&events).unwrap(); assert_eq!(a.version, 7); assert_eq!(a.balance, 0); assert_eq!(a.status, Status::Closed);
}

// R7, R11, R15
#[test]
fn h_unit_limit() {
assert_eq!(replay(&[o(1), d(1), w(1, 0)]).unwrap().balance, 0);
}

// R10
#[test]
fn h_nonpositive_deposits() {
for n in [-99, -2, 0] { failure(&[o(5), d(n)], 1, ApplyError::InvalidAmount); }
}

// R12, R13
#[test]
fn h_zero_withdraw() {
failure(&[o(1), w(0, 2)], 1, ApplyError::InvalidAmount);
}

// R13, R15
#[test]
fn h_floor_at_limit() {
failure(&[o(17), d(17), w(1, 17)], 2, ApplyError::FloorViolation);
}

// R4, R17, R18
#[test]
fn h_close_empty_account() {
assert_eq!(replay(&[o(57), c(1)]).unwrap().version, 2);
}

// R7, R8
#[test]
fn h_bad_open_after_balance() {
failure(&[o(57), d(9), o(0)], 2, ApplyError::InvalidLimit);
}

// R9, R19
#[test]
fn h_deposit_after_closed() {
failure(&[o(57), c(1), d(i64::MIN)], 2, ApplyError::Closed);
}

// R4, R17
#[test]
fn h_drain_then_bad_close() {
failure(&[o(57), d(5), w(5, 0), c(2)], 3, ApplyError::CloseVersion { expected: 2, actual: 3 });
}

// R4, R6, R11
#[test]
fn h_repeated_deposits() {
let a = replay(&[o(12), d(4), d(4), d(4)]).unwrap(); assert_eq!(a.balance, 12); assert_eq!(a.version, 4);
}

// R11, R14, R15
#[test]
fn h_max_limit_arithmetic() {
assert_eq!(replay(&[o(1_000_000_000), d(1_000_000_000), w(999_999_999, 1)]).unwrap().balance, 1);
failure(&[o(1_000_000_000), d(1_000_000_000), d(i64::MAX)], 2, ApplyError::LimitExceeded);
}
