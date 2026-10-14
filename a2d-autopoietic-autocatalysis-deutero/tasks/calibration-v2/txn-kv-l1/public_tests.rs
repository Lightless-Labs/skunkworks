// R1, R2–R5, R13
#[test]
fn basic_values() {
    let mut s = Store::new();
    s.set("red", "7").unwrap();
    s.set("blue", "7").unwrap();
    assert_eq!(s.get("red"), Ok(Some("7".into())));
    assert_eq!(s.count("7"), Ok(2));
    assert_eq!(s.delete("blue"), Ok(true));
    assert_eq!(s.get("blue"), Ok(None));
}

// R6–R8
#[test]
fn nested_commit() {
    let mut s = Store::new();
    s.begin().unwrap();
    s.set("q", "a").unwrap();
    s.begin().unwrap();
    s.set("q", "b").unwrap();
    s.commit().unwrap();
    assert_eq!(s.depth(), Ok(1));
    assert_eq!(s.get("q"), Ok(Some("b".into())));
    s.rollback().unwrap();
    assert_eq!(s.get("q"), Ok(None));
}

// R9–R12, R15
#[test]
fn saved_copy() {
    let mut s = Store::new();
    s.set("key", "first").unwrap();
    s.snapshot("saved").unwrap();
    s.set("key", "second").unwrap();
    s.restore("saved").unwrap();
    assert_eq!(s.get("key"), Ok(Some("first".into())));
    s.forget("saved").unwrap();
    assert_eq!(
        s.restore("saved"),
        Err(Error::SnapshotMissing("saved".into()))
    );
}

// R6–R8, R14
#[test]
fn empty_scope() {
    let mut s = Store::new();
    assert_eq!(s.commit(), Err(Error::NoTransaction));
    s.begin().unwrap();
    s.rollback().unwrap();
    assert_eq!(s.depth(), Ok(0));
}
