// R1, R3–R5, R17–R20
#[test]
fn public_ttl() {
    let mut s = Store::new();
    s.set(2, "red", "9", Some(5)).unwrap();
    assert_eq!(s.get(3, "red"), Ok(Some("9".into())));
    assert_eq!(s.count(3, "9"), Ok(1));
    assert_eq!(s.delete(4, "red"), Ok(true));
    assert_eq!(s.get(4, "red"), Ok(None));
}

// R6–R8, R17, R21–R22
#[test]
fn public_scopes() {
    let mut s = Store::new();
    s.begin(1).unwrap();
    s.set(1, "key", "v", None).unwrap();
    s.begin(2).unwrap();
    s.set(2, "key", "w", None).unwrap();
    s.commit(2).unwrap();
    assert_eq!(s.depth(2), Ok(1));
    s.rollback(3).unwrap();
    assert_eq!(s.get(3, "key"), Ok(None));
}

// R9–R12, R15, R23
#[test]
fn public_snapshots() {
    let mut s = Store::new();
    s.set(1, "x", "saved", Some(20)).unwrap();
    s.snapshot(1, "copy").unwrap();
    s.delete(2, "x").unwrap();
    s.restore(2, "copy").unwrap();
    assert_eq!(s.get(2, "x"), Ok(Some("saved".into())));
    s.forget(2, "copy").unwrap();
}

// R18, R24
#[test]
fn public_time_error() {
    let mut s = Store::new();
    s.depth(10).unwrap();
    assert_eq!(
        s.get(9, "x"),
        Err(Error::TimeBackwards {
            previous: 10,
            now: 9
        })
    );
}
