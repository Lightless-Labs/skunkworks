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
// R2, R4
#[test]
fn public_replay() {
assert_eq!(replay(&[o(90),d(20)]).unwrap().balance,20);
}

// R26, R29, R30, R37, R38
#[test]
fn public_store_log_and_snapshot() {
let mut s=Store::new(2).unwrap();s.append("main",Expected::Missing,&[o(90),d(30),w(10,0)]).unwrap();
assert_eq!(s.records(1),vec![r(2,"main",2,d(30)),r(3,"main",3,w(10,0))]);
assert_eq!(s.snapshot("main").unwrap().unwrap().account.version,2);assert_eq!(s.tail("main").unwrap(),Some(vec![w(10,0)]));
assert_eq!(s.history("main",2).unwrap(),Some(vec![w(10,0)]));assert_eq!(s.state("main").unwrap().unwrap().balance,20);
}

// R39, R42, R46, R47, R48
#[test]
fn public_projection() {
let mut p=Projection::new();let es=vec![r(1,"main",1,o(80)),r(2,"main",2,d(25)),r(3,"side",1,o(40))];
assert_eq!(p.ingest(&es),Ok(3));assert_eq!(p.ingest(&es),Ok(0));assert_eq!(p.position(),3);
assert_eq!(p.account("main").unwrap().unwrap().balance,25);assert_eq!(p.totals(),Totals {streams:2,open:2,closed:0,balance:25});
}

// R32, R33, R34
#[test]
fn public_restore() {
let a=replay(&[o(70),d(20)]).unwrap();let snap=sealed(a);let tail=[w(5,0)];
assert_eq!(rebuild(Some(&snap),&tail).unwrap().balance,15);assert_eq!(rebuild(None,&[o(12)]).unwrap().version,1);
}
