// R1, R2–R3, R9, R32–R35, R38
#[test]
fn h_initial_stats() {
    let mut s = Store::new();
    assert_eq!(
        s.stats(4),
        Ok(Stats {
            clock: 4,
            revision: 0,
            keys: 0,
            live_keys: 0,
            versions: 0,
            tombstones: 0,
            active_transactions: 0,
            read_only_transactions: 0,
            staged_writes: 0,
            snapshots: 0
        })
    );
    assert_eq!(s.get(4, None, "missing"), Ok(None));
    assert_eq!(s.count(4, None, ""), Ok(0));
    assert_eq!(s.gc(4), Ok(0));
    let mut b = Store::new();
    assert_eq!(b.stats(0).unwrap().clock, 0);
}

// R3, R5, R10–R12, R22, R35
#[test]
fn h_unknown_and_clock_precedence() {
    let mut s = Store::new();
    assert_eq!(s.get(1, Some(90), ""), Err(Error::UnknownTransaction(90)));
    assert_eq!(
        s.set(1, Some(90), "", "", Some(u64::MAX)),
        Err(Error::UnknownTransaction(90))
    );
    assert_eq!(
        s.delete(1, Some(90), ""),
        Err(Error::UnknownTransaction(90))
    );
    assert_eq!(
        s.scan(1, Some(90), "z", Some("a"), None, 0, 1, None),
        Err(Error::UnknownTransaction(90))
    );
    assert_eq!(
        s.aggregate(1, Some(90), "z", Some("a"), None),
        Err(Error::UnknownTransaction(90))
    );
    assert_eq!(s.count(1, Some(90), ""), Err(Error::UnknownTransaction(90)));
    assert_eq!(s.commit(1, 90), Err(Error::UnknownTransaction(90)));
    assert_eq!(s.rollback(1, 90), Err(Error::UnknownTransaction(90)));
    assert_eq!(
        s.set(0, Some(90), "", "", None),
        Err(Error::TimeBackwards {
            previous: 1,
            now: 0
        })
    );
}

// R4–R5, R13, R17, R35
#[test]
fn h_duplicate_and_reuse() {
    let mut s = Store::new();
    s.begin(0, 0, false).unwrap();
    s.set(0, Some(0), "k", "own", None).unwrap();
    assert_eq!(s.begin(2, 0, true), Err(Error::TransactionExists(0)));
    assert_eq!(s.get(2, Some(0), "k"), Ok(Some("own".into())));
    assert_eq!(s.commit(2, 0), Ok(1));
    assert_eq!(s.get(2, Some(0), "k"), Err(Error::UnknownTransaction(0)));
    s.set(2, None, "k", "fresh", None).unwrap();
    s.begin(2, 0, true).unwrap();
    assert_eq!(s.get(2, Some(0), "k"), Ok(Some("fresh".into())));
    assert_eq!(s.commit(2, 0), Ok(2));
}

// R6–R8, R36–R37
#[test]
fn h_begin_membership() {
    let mut s = Store::new();
    s.set(0, None, "a", "old", None).unwrap();
    s.begin(0, 10, false).unwrap();
    s.delete(0, None, "a").unwrap();
    s.set(0, None, "b", "new", None).unwrap();
    assert_eq!(s.get(0, Some(10), "a"), Ok(Some("old".into())));
    assert_eq!(s.get(0, Some(10), "b"), Ok(None));
    s.gc(0).unwrap();
    assert_eq!(s.get(0, Some(10), "a"), Ok(Some("old".into())));
    assert_eq!(s.get(0, Some(10), "b"), Ok(None));
    assert_eq!(s.get(0, None, "a"), Ok(None));
}

// R6–R8, R32–R33
#[test]
fn h_expired_head_no_fallback() {
    let mut s = Store::new();
    s.set(0, None, "x", "permanent", None).unwrap();
    s.set(1, None, "x", "short", Some(2)).unwrap();
    s.begin(1, 1, true).unwrap();
    assert_eq!(s.get(3, None, "x"), Ok(None));
    assert_eq!(s.get(3, Some(1), "x"), Ok(None));
    let st = s.stats(3).unwrap();
    assert_eq!(
        (
            st.revision,
            st.keys,
            st.live_keys,
            st.versions,
            st.tombstones
        ),
        (2, 1, 0, 2, 0)
    );
}

// R7–R8, R10, R12–R13, R36
#[test]
fn h_private_expiry_and_tombstone() {
    let mut s = Store::new();
    s.set(0, None, "x", "base", None).unwrap();
    s.begin(0, 4, false).unwrap();
    s.set(1, Some(4), "x", "short", Some(1)).unwrap();
    assert_eq!(s.get(2, Some(4), "x"), Ok(None));
    assert_eq!(s.get(2, None, "x"), Ok(Some("base".into())));
    s.set(2, Some(4), "x", "again", None).unwrap();
    assert_eq!(s.delete(2, Some(4), "x"), Ok(true));
    assert_eq!(s.get(2, Some(4), "x"), Ok(None));
    assert_eq!(s.stats(2).unwrap().staged_writes, 1);
    s.rollback(2, 4).unwrap();
    assert_eq!(s.get(2, None, "x"), Ok(Some("base".into())));
}

// R12–R13, R16–R17, R33–R34
#[test]
fn h_coalesced_atomic_commit() {
    let mut s = Store::new();
    s.begin(0, 5, false).unwrap();
    s.set(0, Some(5), "a", "1", Some(2)).unwrap();
    s.set(0, Some(5), "a", "2", Some(2)).unwrap();
    s.set(0, Some(5), "b", "3", None).unwrap();
    s.delete(0, Some(5), "c").unwrap();
    let st = s.stats(0).unwrap();
    assert_eq!((st.revision, st.versions, st.staged_writes), (0, 0, 3));
    assert_eq!(s.commit(2, 5), Ok(1));
    let st = s.stats(2).unwrap();
    assert_eq!(
        (
            st.keys,
            st.live_keys,
            st.versions,
            st.tombstones,
            st.staged_writes
        ),
        (3, 1, 3, 1, 0)
    );
    assert_eq!(s.get(2, None, "a"), Ok(None));
    s.begin(2, 9, true).unwrap();
    s.set(2, None, "d", "4", None).unwrap();
    assert_eq!(s.commit(2, 9), Ok(2));
}

// R10, R12–R13, R32–R33
#[test]
fn h_absent_delete_zero() {
    let mut s = Store::new();
    assert_eq!(s.delete(0, None, "missing"), Ok(false));
    s.set(0, None, "zero", "discard", Some(0)).unwrap();
    s.set(0, None, "live", "", None).unwrap();
    s.set(0, None, "live", "", None).unwrap();
    let st = s.stats(0).unwrap();
    assert_eq!(
        (
            st.revision,
            st.keys,
            st.live_keys,
            st.versions,
            st.tombstones
        ),
        (4, 3, 1, 4, 2)
    );
    assert_eq!(s.count(0, None, ""), Ok(1));
}

// R10–R12, R35
#[test]
fn h_readonly_deadline_validation() {
    let mut s = Store::new();
    s.begin(0, u64::MAX, true).unwrap();
    assert_eq!(
        s.set(1, Some(u64::MAX), "", "", Some(u64::MAX)),
        Err(Error::KeyEmpty)
    );
    assert_eq!(
        s.set(1, Some(u64::MAX), "x", "", Some(u64::MAX)),
        Err(Error::ReadOnly(u64::MAX))
    );
    assert_eq!(
        s.delete(1, Some(u64::MAX), "x"),
        Err(Error::ReadOnly(u64::MAX))
    );
    assert_eq!(s.delete(1, Some(u64::MAX), ""), Err(Error::KeyEmpty));
    assert_eq!(s.get(1, None, ""), Err(Error::KeyEmpty));
    assert_eq!(
        s.set(1, None, "x", "", Some(u64::MAX)),
        Err(Error::DeadlineOverflow)
    );
    assert_eq!(s.stats(1).unwrap().versions, 0);
    s.set(u64::MAX - 1, None, "x", "edge", Some(1)).unwrap();
    assert_eq!(s.get(u64::MAX, None, "x"), Ok(None));
}

// R18, R21, R34
#[test]
fn h_snapshot_committed_only() {
    let mut s = Store::new();
    s.set(1, None, "a", "committed", None).unwrap();
    s.begin(1, 3, false).unwrap();
    s.set(1, Some(3), "a", "private", None).unwrap();
    s.set(1, Some(3), "b", "private", None).unwrap();
    s.snapshot(1, "saved").unwrap();
    s.rollback(1, 3).unwrap();
    s.delete(1, None, "a").unwrap();
    s.restore(1, "saved").unwrap();
    assert_eq!(s.get(1, None, "a"), Ok(Some("committed".into())));
    assert_eq!(s.get(1, None, "b"), Ok(None));
    assert_eq!(s.stats(1).unwrap().snapshots, 1);
}

// R18–R19, R35
#[test]
fn h_restore_error_order() {
    let mut s = Store::new();
    s.snapshot(0, "known").unwrap();
    s.begin(0, 6, true).unwrap();
    assert_eq!(s.restore(1, ""), Err(Error::NameEmpty));
    assert_eq!(
        s.restore(1, "unknown"),
        Err(Error::SnapshotMissing("unknown".into()))
    );
    assert_eq!(s.restore(1, "known"), Err(Error::ActiveTransactions));
    assert_eq!(s.forget(1, ""), Err(Error::NameEmpty));
    assert_eq!(s.snapshot(1, ""), Err(Error::NameEmpty));
    assert_eq!(
        s.forget(1, "unknown"),
        Err(Error::SnapshotMissing("unknown".into()))
    );
    assert_eq!(s.stats(1).unwrap().revision, 0);
    s.rollback(1, 6).unwrap();
    assert_eq!(s.restore(1, "known"), Ok(0));
    for (name, at) in [("A", 0), ("2a", 0), ("a-", 1), ("aé", 1), ("é", 0)] {
        assert_eq!(s.snapshot(1, name), Err(Error::NameInvalid { at }));
        assert_eq!(s.restore(1, name), Err(Error::NameInvalid { at }));
        assert_eq!(s.forget(1, name), Err(Error::NameInvalid { at }));
    }
    s.snapshot(1, "b").unwrap();
    s.snapshot(1, "c_").unwrap();
    assert_eq!(s.snapshot(1, "d"), Err(Error::SnapshotLimit));
    assert_eq!(s.snapshot(1, "a-"), Err(Error::NameInvalid { at: 1 }));
    s.snapshot(1, "b").unwrap();
    s.forget(1, "known").unwrap();
    s.snapshot(1, "d").unwrap();
}

// R8, R18–R20, R32–R33
#[test]
fn h_restore_union_and_ttl() {
    let mut s = Store::new();
    s.set(1, None, "a", "saved", Some(5)).unwrap();
    s.set(1, None, "b", "expired", Some(1)).unwrap();
    s.snapshot(1, "saved").unwrap();
    s.delete(2, None, "a").unwrap();
    s.set(2, None, "c", "remove", None).unwrap();
    assert_eq!(s.restore(3, "saved"), Ok(5));
    assert_eq!(s.get(3, None, "a"), Ok(Some("saved".into())));
    assert_eq!(s.get(3, None, "b"), Ok(None));
    assert_eq!(s.get(3, None, "c"), Ok(None));
    let st = s.stats(3).unwrap();
    assert_eq!(
        (st.keys, st.versions, st.tombstones, st.live_keys),
        (3, 6, 2, 1)
    );
    assert_eq!(s.restore(4, "saved"), Ok(6));
    assert_eq!(s.restore(6, "saved"), Ok(6));
    assert_eq!(s.stats(6).unwrap().versions, 7);
}

// R18, R20–R21, R30–R31
#[test]
fn h_registry_and_gc_independence() {
    let mut s = Store::new();
    s.set(0, None, "a", "one", None).unwrap();
    s.snapshot(0, "name").unwrap();
    s.set(0, None, "a", "two", None).unwrap();
    s.begin(0, 7, false).unwrap();
    s.snapshot(0, "name").unwrap();
    s.rollback(0, 7).unwrap();
    s.set(0, None, "a", "three", None).unwrap();
    assert_eq!(s.gc(0), Ok(2));
    assert_eq!(s.restore(0, "name"), Ok(4));
    assert_eq!(s.get(0, None, "a"), Ok(Some("two".into())));
    s.begin(0, 8, false).unwrap();
    s.forget(0, "name").unwrap();
    s.rollback(0, 8).unwrap();
    assert_eq!(
        s.restore(0, "name"),
        Err(Error::SnapshotMissing("name".into()))
    );
}

// R14, R16–R17, R36
#[test]
fn h_disjoint_and_read_conflicts() {
    let mut s = Store::new();
    s.set(0, None, "a", "old", None).unwrap();
    s.begin(0, 11, false).unwrap();
    s.begin(0, 12, false).unwrap();
    assert_eq!(s.get(0, Some(11), "a"), Ok(Some("old".into())));
    s.set(0, Some(12), "a", "changed", None).unwrap();
    s.commit(0, 12).unwrap();
    s.set(0, Some(11), "b", "unrelated", None).unwrap();
    assert_eq!(s.commit(0, 11), Ok(3));
    s.begin(0, 13, false).unwrap();
    s.set(0, None, "a", "same", None).unwrap();
    assert_eq!(s.commit(0, 13), Ok(4));
}

// R14–R16, R35–R36
#[test]
fn h_conflict_lex_atomic_retry() {
    let mut s = Store::new();
    s.begin(0, 8, false).unwrap();
    for k in ["z", "a", "m"] {
        s.set(0, Some(8), k, "staged", None).unwrap();
    }
    s.set(0, None, "z", "staged", None).unwrap();
    s.set(0, None, "a", "staged", None).unwrap();
    assert_eq!(
        s.commit(2, 8),
        Err(Error::Conflict {
            tx: 8,
            key: "a".into()
        })
    );
    assert_eq!(s.get(2, None, "m"), Ok(None));
    let st = s.stats(2).unwrap();
    assert_eq!(
        (
            st.revision,
            st.versions,
            st.active_transactions,
            st.staged_writes
        ),
        (2, 2, 1, 3)
    );
    assert_eq!(
        s.commit(3, 8),
        Err(Error::Conflict {
            tx: 8,
            key: "a".into()
        })
    );
    assert_eq!(s.get(3, Some(8), "m"), Ok(Some("staged".into())));
}

// R8, R10, R14–R15, R30–R31, R36
#[test]
fn h_conflicts_tombstones_expiry_gc() {
    for expired in [false, true] {
        let mut s = Store::new();
        s.begin(0, 2, false).unwrap();
        s.set(0, Some(2), "k", "private", Some(1)).unwrap();
        if expired {
            s.set(0, None, "k", "expired", Some(1)).unwrap();
        } else {
            s.delete(0, None, "k").unwrap();
        }
        assert_eq!(s.gc(1), Ok(0));
        assert_eq!(
            s.commit(1, 2),
            Err(Error::Conflict {
                tx: 2,
                key: "k".into()
            })
        );
        assert_eq!(s.stats(1).unwrap().staged_writes, 1);
    }
}

// R6, R30–R34, R37
#[test]
fn h_gc_multi_snapshot_pins() {
    let mut s = Store::new();
    s.set(0, None, "x", "v1", None).unwrap();
    s.begin(0, 1, true).unwrap();
    s.set(0, None, "x", "v2", None).unwrap();
    s.begin(0, 2, false).unwrap();
    s.set(0, None, "x", "v3", None).unwrap();
    s.set(0, None, "x", "v4", None).unwrap();
    assert_eq!(s.gc(0), Ok(1));
    assert_eq!(s.stats(0).unwrap().versions, 3);
    assert_eq!(s.get(0, Some(1), "x"), Ok(Some("v1".into())));
    assert_eq!(s.get(0, Some(2), "x"), Ok(Some("v2".into())));
    assert_eq!(s.gc(0), Ok(0));
    s.commit(0, 1).unwrap();
    assert_eq!(s.gc(0), Ok(1));
    assert_eq!(s.stats(0).unwrap().read_only_transactions, 0);
    s.rollback(0, 2).unwrap();
    assert_eq!(s.gc(0), Ok(1));
    assert_eq!(s.stats(0).unwrap().revision, 4);
}

// R7, R30–R31, R34
#[test]
fn h_gc_own_overwrite_still_pins() {
    let mut s = Store::new();
    s.set(0, None, "k", "old", None).unwrap();
    s.begin(0, 5, false).unwrap();
    s.set(0, Some(5), "k", "private", None).unwrap();
    s.set(0, None, "k", "latest", None).unwrap();
    assert_eq!(s.gc(0), Ok(0));
    let st = s.stats(0).unwrap();
    assert_eq!((st.versions, st.staged_writes), (2, 1));
    assert_eq!(s.get(0, Some(5), "k"), Ok(Some("private".into())));
}

// R8, R30–R33, R37
#[test]
fn h_gc_before_first_and_heads() {
    let mut s = Store::new();
    s.begin(0, 1, true).unwrap();
    s.set(0, None, "a", "v", None).unwrap();
    s.delete(0, None, "a").unwrap();
    s.set(0, None, "b", "expire", Some(1)).unwrap();
    assert_eq!(s.gc(1), Ok(1));
    let st = s.stats(1).unwrap();
    assert_eq!(
        (st.keys, st.live_keys, st.versions, st.tombstones),
        (2, 0, 2, 1)
    );
    assert_eq!(s.get(1, Some(1), "a"), Ok(None));
    assert_eq!(s.gc(1), Ok(0));
    let mut b = Store::new();
    b.set(0, None, "k", "first", None).unwrap();
    b.delete(0, None, "k").unwrap();
    b.begin(0, 2, true).unwrap();
    b.begin(0, 3, false).unwrap();
    b.set(0, None, "k", "last", None).unwrap();
    assert_eq!(b.gc(0), Ok(1));
    assert_eq!(b.stats(0).unwrap().versions, 2);
    assert_eq!(b.stats(0).unwrap().tombstones, 1);
    assert_eq!(b.get(0, Some(2), "k"), Ok(None));
    b.commit(0, 2).unwrap();
    assert_eq!(b.gc(0), Ok(0));
    b.rollback(0, 3).unwrap();
    assert_eq!(b.gc(0), Ok(1));
    assert_eq!(b.stats(0).unwrap().tombstones, 0);
}

// R3, R8, R35
#[test]
fn h_clock_all_operations() {
    let mut s = Store::new();
    s.stats(9).unwrap();
    let e = Err(Error::TimeBackwards {
        previous: 9,
        now: 8,
    });
    assert_eq!(s.begin(8, 0, false), e);
    assert_eq!(s.rollback(8, 0), e);
    assert_eq!(s.set(8, None, "", "", None), e);
    assert_eq!(s.snapshot(8, ""), e);
    assert_eq!(s.forget(8, ""), e);
    assert_eq!(
        s.restore(8, ""),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.commit(8, 0),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.gc(8),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.stats(8),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.delete(8, None, ""),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.get(8, None, ""),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.count(8, None, ""),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.scan(8, None, "z", Some("a"), None, 0, 0, None),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
    assert_eq!(
        s.aggregate(8, None, "z", Some("a"), None),
        Err(Error::TimeBackwards {
            previous: 9,
            now: 8
        })
    );
}

// R4, R7, R17, R21, R34
#[test]
fn h_rollback_separate_transactions() {
    let mut s = Store::new();
    for id in 0..20 {
        s.begin(0, id, id % 2 == 0).unwrap();
        if id % 2 == 1 {
            s.set(0, Some(id), "x", "v", None).unwrap();
        }
    }
    s.snapshot(0, "saved").unwrap();
    let st = s.stats(0).unwrap();
    assert_eq!(
        (
            st.active_transactions,
            st.read_only_transactions,
            st.staged_writes
        ),
        (20, 10, 10)
    );
    s.rollback(0, 1).unwrap();
    assert_eq!(s.get(0, Some(3), "x"), Ok(Some("v".into())));
    assert_eq!(s.get(0, None, "x"), Ok(None));
    assert_eq!(s.stats(0).unwrap().staged_writes, 9);
}

// R7–R9, R22–R24, R27, R30, R37–R38
#[test]
fn h_large_transaction_scans() {
    let mut s = Store::new();
    for i in 0..200 {
        s.set(0, None, &format!("k{i:03}"), "1", None).unwrap();
    }
    s.begin(0, 33, false).unwrap();
    for i in 0..100 {
        s.delete(0, Some(33), &format!("k{i:03}")).unwrap();
    }
    s.set(0, None, "k200", "1", None).unwrap();
    assert_eq!(
        s.aggregate(0, Some(33), "", None, Some("k")),
        Ok(Aggregate {
            count: 100,
            sum: 100
        })
    );
    assert_eq!(
        s.scan(0, Some(33), "", None, None, 0, usize::MAX, None)
            .unwrap()
            .items
            .len(),
        100
    );
    s.gc(0).unwrap();
    assert_eq!(s.commit(0, 33), Ok(202));
    assert_eq!(s.count(0, None, "1"), Ok(101));
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_scan_order_bounds() {
    let mut s = Store::new();
    for k in ["z", "b", "é", "aa", "a"] {
        s.set(0, None, k, k, None).unwrap();
    }
    let p = s
        .scan(0, None, "aa", Some("é"), None, 0, usize::MAX, None)
        .unwrap();
    assert_eq!(
        p.items,
        vec![
            ("aa".into(), "aa".into()),
            ("b".into(), "b".into()),
            ("z".into(), "z".into())
        ]
    );
    assert_eq!(p.next, None);
    s.delete(0, None, "b").unwrap();
    assert_eq!(p.items.len(), 3);
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_scan_prefix() {
    let mut s = Store::new();
    for k in ["pre", "prefix", "p", "prez", "other"] {
        s.set(0, None, k, "x", None).unwrap();
    }
    assert_eq!(
        s.scan(0, None, "pre", Some("prez"), Some("pre"), 0, 9, None)
            .unwrap()
            .items
            .iter()
            .map(|x| x.0.as_str())
            .collect::<Vec<_>>(),
        vec!["pre", "prefix"]
    );
    assert!(
        s.scan(0, None, "z", None, Some("a"), 0, 9, None)
            .unwrap()
            .items
            .is_empty()
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_bad_ranges() {
    let mut s = Store::new();
    let c = Cursor {
        start: "".into(),
        end: None,
        prefix: None,
        offset: 0,
        last: "x".into(),
    };
    assert_eq!(
        s.scan(3, None, "z", Some("a"), None, 0, 0, Some(&c)),
        Err(Error::InvalidRange)
    );
    assert_eq!(
        s.aggregate(3, None, "a", Some("a"), None),
        Err(Error::InvalidRange)
    );
    assert_eq!(
        s.stats(2).map(|st| st.active_transactions),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
    assert_eq!(
        s.scan(2, None, "z", Some("a"), None, 0, 0, Some(&c)),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
    assert_eq!(
        s.aggregate(2, None, "a", Some("a"), None),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_scan_pagination_offset() {
    let mut s = Store::new();
    for k in ["a", "b", "c", "d", "e"] {
        s.set(0, None, k, "v", None).unwrap();
    }
    let p = s.scan(0, None, "", None, None, 2, 1, None).unwrap();
    assert_eq!(p.items[0].0, "c");
    assert_eq!(
        p.next,
        Some(Cursor {
            start: "".into(),
            end: None,
            prefix: None,
            offset: 2,
            last: "c".into()
        })
    );
    let q = s
        .scan(0, None, "", None, None, 2, 8, p.next.as_ref())
        .unwrap();
    assert_eq!(
        q.items.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["d", "e"]
    );
    assert_eq!(q.next, None);
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_zero_huge() {
    let mut s = Store::new();
    s.set(0, None, "x", "7", None).unwrap();
    for (o, l) in [(0, 0), (usize::MAX, usize::MAX), (1, 1)] {
        assert_eq!(
            s.scan(0, None, "", None, None, o, l, None),
            Ok(Page {
                items: vec![],
                next: None
            })
        );
    }
    assert_eq!(
        s.scan(0, None, "", None, None, 0, usize::MAX, None)
            .unwrap()
            .items
            .len(),
        1
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_cursor_binding() {
    let mut s = Store::new();
    let c = Cursor {
        start: "a".into(),
        end: Some("z".into()),
        prefix: Some("".into()),
        offset: 2,
        last: "m".into(),
    };
    assert_eq!(
        s.scan(0, None, "b", Some("z"), Some(""), 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, None, "a", None, Some(""), 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, None, "a", Some("z"), None, 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, None, "a", Some("z"), Some(""), 0, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_cursor_last_validation() {
    let mut s = Store::new();
    for last in ["", "a", "z", "q"] {
        let c = Cursor {
            start: "b".into(),
            end: Some("z".into()),
            prefix: Some("b".into()),
            offset: 0,
            last: last.into(),
        };
        assert_eq!(
            s.scan(0, None, "b", Some("z"), Some("b"), 0, 0, Some(&c)),
            Err(Error::CursorMismatch)
        );
    }
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_cursor_weak_resume() {
    let mut s = Store::new();
    for k in ["b", "d", "f"] {
        s.set(0, None, k, "v", Some(2)).unwrap();
    }
    let p = s.scan(0, None, "", None, None, 0, 1, None).unwrap();
    s.delete(1, None, "b").unwrap();
    s.set(1, None, "a", "v", None).unwrap();
    s.set(1, None, "c", "v", None).unwrap();
    s.set(1, None, "e", "v", None).unwrap();
    let q = s
        .scan(2, None, "", None, None, 0, 9, p.next.as_ref())
        .unwrap();
    assert_eq!(
        q.items.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["c", "e"]
    );
    let mut b = Store::new();
    for k in ["b", "d", "f"] {
        b.set(0, None, k, "v", None).unwrap();
    }
    b.begin(0, 32, false).unwrap();
    let first = b.scan(0, None, "", None, None, 1, 1, None).unwrap();
    b.set(0, Some(32), "e", "v", None).unwrap();
    b.delete(0, Some(32), "d").unwrap();
    let continued = b
        .scan(0, Some(32), "", None, None, 1, 9, first.next.as_ref())
        .unwrap();
    assert_eq!(
        continued
            .items
            .iter()
            .map(|x| x.0.as_str())
            .collect::<Vec<_>>(),
        vec!["e", "f"]
    );
    let mut other = Store::new();
    other.set(0, None, "g", "v", None).unwrap();
    assert_eq!(
        other
            .scan(0, None, "", None, None, 1, 9, first.next.as_ref())
            .unwrap()
            .items,
        vec![("g".into(), "v".into())]
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_aggregate_empty_filtered() {
    let mut s = Store::new();
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Ok(Aggregate { count: 0, sum: 0 })
    );
    s.set(0, None, "bad", "oops", None).unwrap();
    s.set(0, None, "num1", "0", None).unwrap();
    s.set(0, None, "num2", "-7", None).unwrap();
    assert_eq!(
        s.aggregate(0, None, "num", Some("num2"), Some("num")),
        Ok(Aggregate { count: 1, sum: 0 })
    );
    assert_eq!(
        s.aggregate(0, None, "num", None, Some("num")),
        Ok(Aggregate { count: 2, sum: -7 })
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_canonical_rejections() {
    for v in [
        "",
        "+1",
        "01",
        "-0",
        "-01",
        " 1",
        "1 ",
        "１",
        "١",
        "1.0",
        "--1",
        "9223372036854775808",
        "-9223372036854775809",
    ] {
        let mut s = Store::new();
        s.set(0, None, "bad", v, None).unwrap();
        assert_eq!(
            s.aggregate(0, None, "", None, None),
            Err(Error::InvalidNumber {
                key: "bad".into(),
                value: v.into()
            })
        );
        assert_eq!(s.get(0, None, "bad"), Ok(Some(v.into())));
    }
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_numeric_limits() {
    let mut s = Store::new();
    s.set(0, None, "a", &i64::MIN.to_string(), None).unwrap();
    s.set(0, None, "b", &i64::MAX.to_string(), None).unwrap();
    s.set(0, None, "c", "1", None).unwrap();
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Ok(Aggregate { count: 3, sum: 0 })
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_sum_precedence() {
    let mut s = Store::new();
    s.set(0, None, "a", &i64::MAX.to_string(), None).unwrap();
    s.set(0, None, "b", "1", None).unwrap();
    s.set(0, None, "c", "nonsense", None).unwrap();
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Err(Error::SumOverflow { key: "b".into() })
    );
    s.set(0, None, "a", "bad", None).unwrap();
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Err(Error::InvalidNumber {
            key: "a".into(),
            value: "bad".into()
        })
    );
}

// R3, R8–R9, R22–R29, R35, R38
#[test]
fn h_negative_overflow() {
    let mut s = Store::new();
    s.set(0, None, "a", &i64::MIN.to_string(), None).unwrap();
    s.set(0, None, "b", "-1", None).unwrap();
    assert_eq!(
        s.aggregate(0, None, "", None, None),
        Err(Error::SumOverflow { key: "b".into() })
    );
}
