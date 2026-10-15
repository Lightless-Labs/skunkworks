fn o(limit: i64) -> Event { Event::Open { limit } }
fn d(amount: i64) -> Event { Event::Deposit { amount } }
fn w(amount: i64, floor: i64) -> Event { Event::Withdraw { amount, floor } }
fn c(expected_version: u64) -> Event { Event::Close { expected_version } }
fn failure(events: &[Event], index: usize, kind: ApplyError) {
    assert_eq!(replay(events), Err(ReplayError { index, kind }));
}
fn fresh() -> Store { Store::new(3).unwrap() }
fn sealed(account: Account) -> Snapshot { Snapshot { tag: snapshot_tag(&account), account } }
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

// R28, R29, R30, R36
#[test]
fn public_checkpoint_and_tail() {
let mut s=Store::new(2).unwrap();let es=vec![o(90),d(30),w(10,0)];s.append("main",Expected::Missing,&es).unwrap();
let snap=s.snapshot("main").unwrap().unwrap();assert_eq!(snap.account,replay(&es[..2]).unwrap());
assert_eq!(s.tail("main").unwrap(),Some(vec![w(10,0)]));assert_eq!(s.state("main").unwrap().unwrap(),replay(&es).unwrap());
}

// R32, R33, R34
#[test]
fn public_restore() {
let a=replay(&[o(70),d(20)]).unwrap();let snap=sealed(a);let tail=[w(5,0)];
assert_eq!(rebuild(Some(&snap),&tail).unwrap().balance,15);assert_eq!(rebuild(None,&[o(12)]).unwrap().version,1);
}
