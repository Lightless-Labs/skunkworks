fn o(limit: i64) -> Event { Event::Open { limit } }
fn d(amount: i64) -> Event { Event::Deposit { amount } }
fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
fn failure(events: &[Event], index: usize, kind: ApplyError) {
    assert_eq!(replay(events), Err(ReplayError { index, kind }));
}
fn fresh() -> Store { Store::new() }
// R2, R4
#[test]
fn public_replay() {
assert_eq!(replay(&[o(90),d(20)]).unwrap().balance,20);
}

// R22, R24, R26
#[test]
fn public_append_and_read() {
let mut s=fresh();s.append("main",Expected::Missing,&[o(90),d(30)]).unwrap();
assert_eq!(s.append("main",Expected::Exact(2),&[w(10,0)]).unwrap().balance,20);
assert_eq!(s.state("main").unwrap().unwrap().version,3);assert_eq!(s.history("main",2).unwrap(),Some(vec![w(10,0)]));
}

// R22, R27
#[test]
fn public_two_streams() {
let mut s=fresh();s.append("left",Expected::Any,&[o(100)]).unwrap();s.append("right",Expected::Missing,&[o(50),d(5)]).unwrap();
assert_eq!(s.state("left").unwrap().unwrap().balance,0);assert_eq!(s.state("right").unwrap().unwrap().balance,5);
}

// R10, R24
#[test]
fn public_reject_invalid_event() {
let mut s=fresh();s.append("bank",Expected::Any,&[o(100)]).unwrap();
assert_eq!(s.append("bank",Expected::Any,&[d(0)]),Err(StoreError::Rejected(ReplayError {index:0,kind:ApplyError::InvalidAmount})));
}
