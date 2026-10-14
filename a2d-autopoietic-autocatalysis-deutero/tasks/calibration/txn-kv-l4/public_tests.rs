#[test]
fn public_timed_values() {
    let mut s = Store::new();
    s.set("session", "open", Some(5), 0).unwrap();
    s.set("config", "fixed", None, 0).unwrap();
    assert_eq!(s.get("session", 4), Ok(Some("open".into())));
    assert_eq!(s.count("open", 4), Ok(1));
    assert_eq!(s.get("session", 6), Ok(None));
    assert_eq!(s.delete("config", 6), Ok(true));
}

#[test]
fn public_timed_rollback_commit() {
    let mut s = Store::new();
    s.set("setting", "original", None, 1).unwrap();
    s.begin(2).unwrap();
    s.set("setting", "changed", None, 3).unwrap();
    s.rollback(4).unwrap();
    assert_eq!(s.get("setting", 4), Ok(Some("original".into())));
    s.begin(4).unwrap();
    s.set("saved", "kept", None, 4).unwrap();
    s.commit(4).unwrap();
    assert_eq!(s.get("saved", 4), Ok(Some("kept".into())));
}

#[test]
fn public_timed_snapshot() {
    let mut s = Store::new();
    s.set("draft", "v-public", Some(10), 1).unwrap();
    s.snapshot(5, 2).unwrap();
    s.delete("draft", 2).unwrap();
    s.restore(5, 3).unwrap();
    assert_eq!(s.get("draft", 3), Ok(Some("v-public".into())));
}

#[test]
fn public_backward_clock() {
    let mut s = Store::new();
    assert_eq!(s.count("unused", 8), Ok(0));
    assert_eq!(
        s.get("unused", 7),
        Err(Error::ClockWentBackwards { last: 8, now: 7 })
    );
    assert_eq!(s.get("unused", 8), Ok(None));
}
