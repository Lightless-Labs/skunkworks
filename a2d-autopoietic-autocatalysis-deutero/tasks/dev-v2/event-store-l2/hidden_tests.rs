fn o(limit: i64) -> Event { Event::Open { limit } }
fn d(amount: i64) -> Event { Event::Deposit { amount } }
fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
fn failure(events: &[Event], index: usize, kind: ApplyError) {
    assert_eq!(replay(events), Err(ReplayError { index, kind }));
}
fn fresh() -> Store { Store::new() }
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

// R21
#[test]
fn h_id_bytes() {
let mut s = fresh();
for (id, byte) in [("",0), ("0abc",0), ("-abc",0), ("Abc",0), ("a_b",1), ("abé",2), ("a😀",1), ("abcdefghijklmnopqrstuvwxy",24), ("abc!abcdefghijklmnopqrstuX",3)] {
    let e = StoreError::InvalidId { byte };
    assert_eq!(s.state(id), Err(e.clone())); assert_eq!(s.history(id,0), Err(e.clone()));
    assert_eq!(s.append(id, Expected::Exact(99), &[d(-1)]), Err(e));
}
for id in ["a", "a0--9z", "abcdefghijklmnopqrstuvwx"] { s.append(id, Expected::Missing, &[o(31)]).unwrap(); }
}

// R22, R23
#[test]
fn h_conflict_values() {
let mut s = fresh();
assert_eq!(s.append("missing",Expected::Exact(0),&[]), Err(StoreError::Conflict { expected: Expected::Exact(0), actual: None }));
s.append("x",Expected::Missing,&[o(20)]).unwrap();
assert_eq!(s.append("x",Expected::Missing,&[d(-1)]), Err(StoreError::Conflict { expected: Expected::Missing, actual: Some(1) }));
assert_eq!(s.append("x",Expected::Exact(0),&[d(-1)]), Err(StoreError::Conflict { expected: Expected::Exact(0), actual: Some(1) }));
assert_eq!(s.append("x",Expected::Exact(9),&[]), Err(StoreError::Conflict { expected: Expected::Exact(9), actual: Some(1) }));
assert_eq!(s.state("missing").unwrap(),None);
}

// R23, R24
#[test]
fn h_append_rollback_existing() {
let mut s = fresh(); s.append("roll",Expected::Any,&[o(50),d(7)]).unwrap();
let old=s.clone();
assert_eq!(s.append("roll",Expected::Exact(2),&[d(8),w(100,0),d(1)]), Err(StoreError::Rejected(ReplayError { index:1,kind:ApplyError::InsufficientFunds })));
assert_eq!(s,old);
assert_eq!(s.append("roll",Expected::Exact(2),&[w(7,0),c(3)]).unwrap().status,Status::Closed);
}

// R23, R24, R27
#[test]
fn h_append_rollback_missing() {
let mut s = fresh(); let old=s.clone();
assert_eq!(s.append("gone",Expected::Missing,&[o(90),d(3),w(0,-1)]), Err(StoreError::Rejected(ReplayError { index:2,kind:ApplyError::InvalidAmount })));
assert_eq!(s,old); assert_eq!(s.state("gone").unwrap(),None);
s.append("gone",Expected::Missing,&[o(90)]).unwrap();
}

// R22, R25, R26, R27
#[test]
fn h_empty_never_materializes() {
let mut s=fresh(); let old=s.clone();
assert_eq!(s.append("void",Expected::Any,&[]).unwrap(), replay(&[]).unwrap()); assert_eq!(s,old);
s.append("void",Expected::Missing,&[]).unwrap(); assert_eq!(s.state("void").unwrap(),None);
assert_eq!(s.history("void",0).unwrap(),None);
s.append("void",Expected::Missing,&[o(4)]).unwrap(); let saved=s.clone();
assert_eq!(s.append("void",Expected::Exact(1),&[]).unwrap().version,1); assert_eq!(s,saved);
}

// R26, R27
#[test]
fn h_history_bounds() {
let mut s=fresh(); let es=vec![o(22),d(8),w(8,0),c(3)];
s.append("hist",Expected::Any,&es).unwrap();
for after in 0..=4 { assert_eq!(s.history("hist",after).unwrap(),Some(es[after as usize..].to_vec())); }
assert_eq!(s.history("hist",5).unwrap(),Some(vec![])); assert_eq!(s.history("hist",u64::MAX).unwrap(),Some(vec![]));
assert_eq!(s.history("other",0).unwrap(),None); assert_eq!(s.state("hist").unwrap(),Some(replay(&es).unwrap()));
}

// R22, R27
#[test]
fn h_stream_isolation() {
let mut s=fresh();
s.append("a",Expected::Missing,&[o(80),d(13)]).unwrap(); s.append("b",Expected::Exact(0),&[]).unwrap_err();
s.append("b",Expected::Missing,&[o(7)]).unwrap(); s.append("a",Expected::Exact(2),&[d(2)]).unwrap();
assert_eq!(s.state("a").unwrap().unwrap().balance,15); assert_eq!(s.state("b").unwrap().unwrap().version,1);
s.append("b",Expected::Exact(1),&[c(1)]).unwrap(); assert_eq!(s.state("a").unwrap().unwrap().version,3);
}

// R20, R26, R27
#[test]
fn h_owned_history_state() {
let mut s=fresh(); let input=vec![o(15),d(4)]; let backup=input.clone();
s.append("copy",Expected::Any,&input).unwrap(); assert_eq!(input,backup);
let mut a=s.state("copy").unwrap().unwrap(); a.balance=999;
let mut h=s.history("copy",0).unwrap().unwrap(); h[0]=o(1);
let mut cloned=s.clone(); cloned.append("copy",Expected::Any,&[d(1)]).unwrap();
assert_eq!(s.state("copy").unwrap().unwrap().balance,4); assert_eq!(s.history("copy",0).unwrap(),Some(input)); assert_ne!(s,cloned);
}

// R4, R17, R24
#[test]
fn h_batch_local_version() {
let mut s=fresh(); s.append("ver",Expected::Any,&[o(25),d(3),w(3,0)]).unwrap();
assert_eq!(s.append("ver",Expected::Exact(3),&[c(2)]),Err(StoreError::Rejected(ReplayError {index:0,kind:ApplyError::CloseVersion {expected:2,actual:3}})));
assert_eq!(s.append("ver",Expected::Exact(3),&[d(2),w(2,0),c(5)]).unwrap().version,6);
}

// R7, R9, R16, R22, R24, R27
#[test]
fn h_closed_store_errors() {
let mut s=fresh(); s.append("shut",Expected::Any,&[o(60),c(1)]).unwrap();
assert_eq!(s.append("shut",Expected::Exact(1),&[o(0)]),Err(StoreError::Conflict {expected:Expected::Exact(1),actual:Some(2)}));
assert_eq!(s.append("shut",Expected::Any,&[o(0)]),Err(StoreError::Rejected(ReplayError {index:0,kind:ApplyError::InvalidLimit})));
assert_eq!(s.append("shut",Expected::Any,&[d(0)]),Err(StoreError::Rejected(ReplayError {index:0,kind:ApplyError::Closed})));
assert_eq!(s.append("shut",Expected::Exact(2),&[]).unwrap().status,Status::Closed);
}

// R4, R23, R24, R26
#[test]
fn h_large_atomic_batch() {
let mut s=fresh(); let mut es=vec![o(500)]; for _ in 0..200 {es.push(d(1));}
s.append("large",Expected::Any,&es).unwrap(); let old=s.clone();
let mut bad=vec![w(1,0); 200]; bad.push(w(1,0));
assert_eq!(s.append("large",Expected::Exact(201),&bad),Err(StoreError::Rejected(ReplayError {index:200,kind:ApplyError::InsufficientFunds}))); assert_eq!(s,old);
assert_eq!(s.history("large",200).unwrap(),Some(vec![d(1)]));
}

// R21, R22, R25, R26
#[test]
fn h_id_reads_precede_absence() {
let mut s=fresh();
assert_eq!(s.append("Bad",Expected::Any,&[]),Err(StoreError::InvalidId {byte:0}));
assert_eq!(s.state("a/"),Err(StoreError::InvalidId {byte:1}));
assert_eq!(s.history("a/",u64::MAX),Err(StoreError::InvalidId {byte:1}));
assert_eq!(s.state("valid").unwrap(),None);
}
