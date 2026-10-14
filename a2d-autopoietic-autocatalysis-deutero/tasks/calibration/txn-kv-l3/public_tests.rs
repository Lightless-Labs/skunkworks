#[test]
fn public_visible_core() {
    let mut s = Store::new();
    s.set("record", "silver");
    assert_eq!(s.get("record"), Some("silver".into()));
    assert_eq!(s.count("silver"), 1);
    assert!(s.delete("record"));
}

#[test]
fn public_nested_counts() {
    let mut s = Store::new();
    s.set("progress", "base-public");
    s.begin();
    s.set("progress", "outer-public");
    s.begin();
    s.set("progress", "inner-public");
    assert_eq!(s.count("inner-public"), 1);
    s.rollback().unwrap();
    assert_eq!(s.count("outer-public"), 1);
    s.commit().unwrap();
    assert_eq!(s.get("progress"), Some("outer-public".into()));
}

#[test]
fn public_snapshot_roundtrip() {
    let mut s = Store::new();
    s.set("document", "draft");
    s.snapshot(7);
    s.set("document", "revised");
    assert_eq!(s.restore(7), Ok(()));
    assert_eq!(s.get("document"), Some("draft".into()));
}

#[test]
fn public_missing_snapshot() {
    let mut s = Store::new();
    assert_eq!(s.restore(99), Err(Error::UnknownSnapshot(99)));
    assert_eq!(s.commit(), Err(Error::NoTransaction));
    assert_eq!(s.rollback(), Err(Error::NoTransaction));
}
