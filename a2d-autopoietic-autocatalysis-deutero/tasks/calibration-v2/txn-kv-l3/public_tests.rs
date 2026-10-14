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

// R25–R30
#[test]
fn public_pages() {
    let mut s = Store::new();
    for (k, v) in [("aa", "1"), ("ab", "2"), ("ac", "3")] {
        s.set(0, k, v, None).unwrap();
    }
    let p = s.scan(0, "", None, Some("a"), 0, 2, None).unwrap();
    assert_eq!(
        p.items,
        vec![("aa".into(), "1".into()), ("ab".into(), "2".into())]
    );
    let q = s
        .scan(0, "", None, Some("a"), 0, 2, p.next.as_ref())
        .unwrap();
    assert_eq!(q.items, vec![("ac".into(), "3".into())]);
    assert_eq!(q.next, None);
}

// R9–R12, R23, R31–R33
#[test]
fn public_numbers_and_save() {
    let mut s = Store::new();
    s.set(1, "n1", "4", None).unwrap();
    s.set(1, "n2", "-2", None).unwrap();
    s.snapshot(1, "saved").unwrap();
    assert_eq!(
        s.aggregate(1, "", None, None),
        Ok(Aggregate { count: 2, sum: 2 })
    );
    s.delete(2, "n2").unwrap();
    s.restore(2, "saved").unwrap();
    assert_eq!(s.count(2, "-2"), Ok(1));
    s.forget(2, "saved").unwrap();
}
