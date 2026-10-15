fn o(limit: i64) -> Event { Event::Open { limit } }
fn d(amount: i64) -> Event { Event::Deposit { amount } }
fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
fn failure(events: &[Event], index: usize, kind: ApplyError) {
    assert_eq!(replay(events), Err(ReplayError { index, kind }));
}
// R2, R4, R8
#[test]
fn public_open() {
assert_eq!(replay(&[o(60)]).unwrap(),Account {status:Status::Open,balance:0,limit:60,version:1});
}

// R10, R11, R12, R15
#[test]
fn public_money_movement() {
let a=replay(&[o(60),d(40),w(15,0)]).unwrap();assert_eq!(a.balance,25);assert_eq!(a.version,3);
}

// R17, R18, R19
#[test]
fn public_close() {
let a=replay(&[o(60),d(20),w(20,0),c(3)]).unwrap();assert_eq!(a.status,Status::Closed);assert_eq!(a.limit,60);
}
