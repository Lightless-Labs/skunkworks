// R1, R4–R7, R9, R13, R16
#[test]
fn public_isolation() {
    let mut s = Store::new();
    s.set(1, None, "key", "base", None).unwrap();
    s.begin(1, 7, false).unwrap();
    s.set(2, None, "key", "later", None).unwrap();
    assert_eq!(s.get(2, Some(7), "key"), Ok(Some("base".into())));
    s.set(2, Some(7), "other", "own", None).unwrap();
    assert_eq!(s.count(2, Some(7), "own"), Ok(1));
    assert_eq!(s.commit(2, 7), Ok(3));
}

// R11, R14–R17
#[test]
fn public_conflict_and_readonly() {
    let mut s = Store::new();
    s.begin(0, 1, false).unwrap();
    s.begin(0, 2, false).unwrap();
    s.set(0, Some(1), "x", "a", None).unwrap();
    s.set(0, Some(2), "x", "b", None).unwrap();
    assert_eq!(s.commit(0, 1), Ok(1));
    assert_eq!(
        s.commit(0, 2),
        Err(Error::Conflict {
            tx: 2,
            key: "x".into()
        })
    );
    s.rollback(0, 2).unwrap();
    s.begin(0, 3, true).unwrap();
    assert_eq!(s.set(0, Some(3), "x", "c", None), Err(Error::ReadOnly(3)));
    assert_eq!(s.commit(0, 3), Ok(1));
}

// R22–R29
#[test]
fn public_scan_aggregate() {
    let mut s = Store::new();
    for (k, v) in [("a", "2"), ("b", "-1"), ("c", "4")] {
        s.set(0, None, k, v, None).unwrap();
    }
    let p = s.scan(0, None, "", None, None, 0, 2, None).unwrap();
    assert_eq!(p.items.len(), 2);
    assert_eq!(
        s.scan(0, None, "", None, None, 0, 2, p.next.as_ref())
            .unwrap()
            .items,
        vec![("c".into(), "4".into())]
    );
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Ok(Aggregate { count: 3, sum: 5 })
    );
}

// R8, R10, R18–R21, R30–R34
#[test]
fn public_snapshots_gc_stats() {
    let mut s = Store::new();
    s.set(1, None, "x", "old", Some(20)).unwrap();
    s.snapshot(1, "saved").unwrap();
    s.set(2, None, "x", "new", None).unwrap();
    assert_eq!(s.restore(2, "saved"), Ok(3));
    assert_eq!(s.get(2, None, "x"), Ok(Some("old".into())));
    assert_eq!(s.delete(2, None, "x"), Ok(true));
    assert_eq!(s.gc(2), Ok(3));
    let st = s.stats(2).unwrap();
    assert_eq!(
        (st.revision, st.versions, st.tombstones, st.live_keys),
        (4, 1, 1, 0)
    );
    s.forget(2, "saved").unwrap();
}
