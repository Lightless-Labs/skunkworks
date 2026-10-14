// R1, R13–R14, R17–R24
#[test]
fn h_key_errors() {
    let mut s = Store::new();
    assert_eq!(s.set(0, "", "v", None), Err(Error::KeyEmpty));
    assert_eq!(s.get(0, ""), Err(Error::KeyEmpty));
    assert_eq!(s.delete(0, ""), Err(Error::KeyEmpty));
    assert_eq!(s.count(0, "v"), Ok(0));
}

// R11–R16, R17–R24
#[test]
fn h_names() {
    let mut s = Store::new();
    assert_eq!(s.snapshot(0, ""), Err(Error::NameEmpty));
    assert_eq!(s.restore(0, ""), Err(Error::NameEmpty));
    assert_eq!(s.forget(0, ""), Err(Error::NameEmpty));
    assert_eq!(
        s.restore(0, "lost"),
        Err(Error::SnapshotMissing("lost".into()))
    );
    assert_eq!(
        s.forget(0, "lost"),
        Err(Error::SnapshotMissing("lost".into()))
    );
    for (name, at) in [
        ("A", 0),
        ("2a", 0),
        ("a-", 1),
        ("aé", 1),
        ("é", 0),
        ("aa b", 2),
    ] {
        assert_eq!(s.snapshot(0, name), Err(Error::NameInvalid { at }));
        assert_eq!(s.restore(0, name), Err(Error::NameInvalid { at }));
        assert_eq!(s.forget(0, name), Err(Error::NameInvalid { at }));
    }
    for name in ["a", "b2", "c_"] {
        s.snapshot(0, name).unwrap();
    }
    assert_eq!(s.snapshot(0, "d"), Err(Error::SnapshotLimit));
    assert_eq!(s.snapshot(0, "a-"), Err(Error::NameInvalid { at: 1 }));
    s.snapshot(0, "b2").unwrap();
    s.begin(0).unwrap();
    s.forget(0, "a").unwrap();
    s.rollback(0).unwrap();
    s.snapshot(0, "d").unwrap();
    assert_eq!(s.restore(0, "a"), Err(Error::SnapshotMissing("a".into())));
    s.set(0, "quota", "replacement", None).unwrap();
    s.snapshot(0, "b2").unwrap();
    s.delete(0, "quota").unwrap();
    s.restore(0, "b2").unwrap();
    assert_eq!(s.get(0, "quota"), Ok(Some("replacement".into())));
}

// R6, R14, R17–R24
#[test]
fn h_depth_limit() {
    let mut s = Store::new();
    for _ in 0..4 {
        s.begin(0).unwrap();
    }
    assert_eq!(s.begin(0), Err(Error::DepthLimit));
    assert_eq!(s.depth(0), Ok(4));
    for _ in 0..4 {
        s.commit(0).unwrap();
    }
    assert_eq!(s.depth(0), Ok(0));
}

// R7–R8, R14, R17–R24
#[test]
fn h_no_scopes() {
    let mut s = Store::new();
    s.set(0, "x", "v", None).unwrap();
    assert_eq!(s.rollback(0), Err(Error::NoTransaction));
    assert_eq!(s.commit(0), Err(Error::NoTransaction));
    assert_eq!(s.get(0, "x"), Ok(Some("v".into())));
}

// R3, R5, R17–R24
#[test]
fn h_working_count() {
    let mut s = Store::new();
    s.set(0, "x", "yes", None).unwrap();
    s.begin(0).unwrap();
    s.set(0, "x", "no", None).unwrap();
    s.set(0, "y", "no", None).unwrap();
    assert_eq!(s.count(0, "yes"), Ok(0));
    assert_eq!(s.count(0, "no"), Ok(2));
}

// R7–R8, R17–R24
#[test]
fn h_nested_commit_undo() {
    let mut s = Store::new();
    s.set(0, "x", "base", None).unwrap();
    s.begin(0).unwrap();
    s.begin(0).unwrap();
    s.delete(0, "x").unwrap();
    s.commit(0).unwrap();
    assert_eq!(s.get(0, "x"), Ok(None));
    s.rollback(0).unwrap();
    assert_eq!(s.get(0, "x"), Ok(Some("base".into())));
}

// R9–R10, R17–R24
#[test]
fn h_restore_depth() {
    let mut s = Store::new();
    s.set(0, "x", "saved", None).unwrap();
    s.snapshot(0, "s").unwrap();
    s.begin(0).unwrap();
    s.begin(0).unwrap();
    s.restore(0, "s").unwrap();
    assert_eq!(s.depth(0), Ok(2));
    s.rollback(0).unwrap();
    assert_eq!(s.depth(0), Ok(1));
}

// R8–R10, R17–R24
#[test]
fn h_restore_parent() {
    let mut s = Store::new();
    s.set(0, "x", "snapshot", None).unwrap();
    s.snapshot(0, "s").unwrap();
    s.set(0, "x", "parent", None).unwrap();
    s.begin(0).unwrap();
    s.restore(0, "s").unwrap();
    assert_eq!(s.get(0, "x"), Ok(Some("snapshot".into())));
    s.rollback(0).unwrap();
    assert_eq!(s.get(0, "x"), Ok(Some("parent".into())));
}

// R8–R9, R15, R17–R24
#[test]
fn h_snapshot_survives() {
    let mut s = Store::new();
    s.begin(0).unwrap();
    s.set(0, "x", "ghost", None).unwrap();
    s.snapshot(0, "s").unwrap();
    s.rollback(0).unwrap();
    assert_eq!(s.get(0, "x"), Ok(None));
    s.restore(0, "s").unwrap();
    assert_eq!(s.get(0, "x"), Ok(Some("ghost".into())));
}

// R9–R10, R15, R17–R24
#[test]
fn h_saved_immutable() {
    let mut s = Store::new();
    s.set(0, "x", "original", None).unwrap();
    s.snapshot(0, "s").unwrap();
    s.restore(0, "s").unwrap();
    s.set(0, "x", "edited", None).unwrap();
    s.restore(0, "s").unwrap();
    assert_eq!(s.get(0, "x"), Ok(Some("original".into())));
}

// R11, R14, R17–R24
#[test]
fn h_missing_restore_atomic() {
    let mut s = Store::new();
    s.begin(0).unwrap();
    s.set(0, "x", "present", None).unwrap();
    assert_eq!(
        s.restore(0, "missing"),
        Err(Error::SnapshotMissing("missing".into()))
    );
    assert_eq!(s.depth(0), Ok(1));
    assert_eq!(s.get(0, "x"), Ok(Some("present".into())));
}

// R14, R17–R24
#[test]
fn h_independent() {
    let mut a = Store::new();
    let mut b = Store::new();
    a.set(0, "x", "only-a", None).unwrap();
    a.snapshot(0, "s").unwrap();
    assert_eq!(b.get(0, "x"), Ok(None));
    assert_eq!(b.restore(0, "s"), Err(Error::SnapshotMissing("s".into())));
}

// R2–R5, R9–R12, R15–R24
#[test]
fn h_core_maps() {
    let mut s = Store::new();
    assert_eq!(s.get(0, "missing"), Ok(None));
    assert_eq!(s.delete(0, "missing"), Ok(false));
    for k in ["é", "e\u{301}", " "] {
        s.set(0, k, "", None).unwrap();
    }
    assert_eq!(s.count(0, ""), Ok(3));
    s.set(0, "é", "1", None).unwrap();
    assert_eq!(s.count(0, ""), Ok(2));
    s.snapshot(0, "saved").unwrap();
    s.delete(0, "é").unwrap();
    s.restore(0, "saved").unwrap();
    assert_eq!(s.get(0, "é"), Ok(Some("1".into())));
    s.forget(0, "saved").unwrap();
}

// R4, R20
#[test]
fn h_zero_and_permanent() {
    let mut s = Store::new();
    s.set(5, "k", "x", None).unwrap();
    s.set(5, "k", "gone", Some(0)).unwrap();
    assert_eq!(s.get(5, "k"), Ok(None));
    s.set(5, "k", "short", Some(1)).unwrap();
    s.set(5, "k", "long", None).unwrap();
    assert_eq!(s.get(u64::MAX, "k"), Ok(Some("long".into())));
}

// R13, R18–R20
#[test]
fn h_overflow_precedence() {
    let mut s = Store::new();
    s.set(0, "k", "old", None).unwrap();
    assert_eq!(
        s.set(u64::MAX, "k", "bad", Some(1)),
        Err(Error::DeadlineOverflow)
    );
    assert_eq!(s.get(u64::MAX, "k"), Ok(Some("old".into())));
    assert_eq!(s.set(u64::MAX, "", "bad", Some(1)), Err(Error::KeyEmpty));
    assert_eq!(
        s.set(0, "", "bad", Some(1)),
        Err(Error::TimeBackwards {
            previous: u64::MAX,
            now: 0
        })
    );
}

// R12, R18–R19, R24
#[test]
fn h_clock_errors() {
    let mut s = Store::new();
    s.set(1, "x", "v", Some(4)).unwrap();
    assert_eq!(s.restore(5, ""), Err(Error::NameEmpty));
    assert_eq!(
        s.get(4, "x"),
        Err(Error::TimeBackwards {
            previous: 5,
            now: 4
        })
    );
    assert_eq!(s.get(5, "x"), Ok(None));
    assert_eq!(s.commit(6), Err(Error::NoTransaction));
    assert_eq!(
        s.rollback(5),
        Err(Error::TimeBackwards {
            previous: 6,
            now: 5
        })
    );
    assert_eq!(s.snapshot(6, ""), Err(Error::NameEmpty));
    assert_eq!(s.forget(6, ""), Err(Error::NameEmpty));
}

// R8, R22
#[test]
fn h_rollback_live_original() {
    let mut s = Store::new();
    s.set(2, "k", "old", Some(8)).unwrap();
    s.begin(3).unwrap();
    s.set(3, "k", "new", Some(1)).unwrap();
    assert_eq!(s.get(4, "k"), Ok(None));
    s.rollback(5).unwrap();
    assert_eq!(s.get(9, "k"), Ok(Some("old".into())));
    assert_eq!(s.get(10, "k"), Ok(None));
    s.set(10, "k", "old", Some(2)).unwrap();
    s.begin(10).unwrap();
    s.set(10, "k", "new", None).unwrap();
    s.rollback(12).unwrap();
    assert_eq!(s.get(12, "k"), Ok(None));
}

// R7, R21
#[test]
fn h_commit_deadline() {
    let mut s = Store::new();
    s.begin(3).unwrap();
    s.set(3, "k", "v", Some(5)).unwrap();
    s.begin(4).unwrap();
    s.commit(5).unwrap();
    s.commit(6).unwrap();
    assert_eq!(s.get(8, "k"), Ok(None));
}

// R9–R10, R15, R23
#[test]
fn h_snapshot_absolute() {
    let mut s = Store::new();
    s.set(3, "k", "v", Some(5)).unwrap();
    s.snapshot(4, "s").unwrap();
    s.delete(5, "k").unwrap();
    s.restore(6, "s").unwrap();
    assert_eq!(s.get(7, "k"), Ok(Some("v".into())));
    assert_eq!(s.get(8, "k"), Ok(None));
    s.restore(9, "s").unwrap();
    assert_eq!(s.get(9, "k"), Ok(None));
}

// R25–R26, R34
#[test]
fn h_scan_order_bounds() {
    let mut s = Store::new();
    for k in ["z", "b", "é", "aa", "a"] {
        s.set(0, k, k, None).unwrap();
    }
    let p = s
        .scan(0, "aa", Some("é"), None, 0, usize::MAX, None)
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
    s.delete(0, "b").unwrap();
    assert_eq!(p.items.len(), 3);
}

// R25–R26
#[test]
fn h_scan_prefix() {
    let mut s = Store::new();
    for k in ["pre", "prefix", "p", "prez", "other"] {
        s.set(0, k, "x", None).unwrap();
    }
    assert_eq!(
        s.scan(0, "pre", Some("prez"), Some("pre"), 0, 9, None)
            .unwrap()
            .items
            .iter()
            .map(|x| x.0.as_str())
            .collect::<Vec<_>>(),
        vec!["pre", "prefix"]
    );
    assert!(
        s.scan(0, "z", None, Some("a"), 0, 9, None)
            .unwrap()
            .items
            .is_empty()
    );
}

// R18, R26, R29, R34
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
        s.scan(3, "z", Some("a"), None, 0, 0, Some(&c)),
        Err(Error::InvalidRange)
    );
    assert_eq!(
        s.aggregate(3, "a", Some("a"), None),
        Err(Error::InvalidRange)
    );
    assert_eq!(
        s.depth(2),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
    assert_eq!(
        s.scan(2, "z", Some("a"), None, 0, 0, Some(&c)),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
    assert_eq!(
        s.aggregate(2, "a", Some("a"), None),
        Err(Error::TimeBackwards {
            previous: 3,
            now: 2
        })
    );
}

// R27–R30
#[test]
fn h_scan_pagination_offset() {
    let mut s = Store::new();
    for k in ["a", "b", "c", "d", "e"] {
        s.set(0, k, "v", None).unwrap();
    }
    let p = s.scan(0, "", None, None, 2, 1, None).unwrap();
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
    let q = s.scan(0, "", None, None, 2, 8, p.next.as_ref()).unwrap();
    assert_eq!(
        q.items.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["d", "e"]
    );
    assert_eq!(q.next, None);
}

// R27–R28
#[test]
fn h_zero_huge() {
    let mut s = Store::new();
    s.set(0, "x", "7", None).unwrap();
    for (o, l) in [(0, 0), (usize::MAX, usize::MAX), (1, 1)] {
        assert_eq!(
            s.scan(0, "", None, None, o, l, None),
            Ok(Page {
                items: vec![],
                next: None
            })
        );
    }
    assert_eq!(
        s.scan(0, "", None, None, 0, usize::MAX, None)
            .unwrap()
            .items
            .len(),
        1
    );
}

// R29
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
        s.scan(0, "b", Some("z"), Some(""), 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, "a", None, Some(""), 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, "a", Some("z"), None, 2, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
    assert_eq!(
        s.scan(0, "a", Some("z"), Some(""), 0, 1, Some(&c)),
        Err(Error::CursorMismatch)
    );
}

// R27, R29
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
            s.scan(0, "b", Some("z"), Some("b"), 0, 0, Some(&c)),
            Err(Error::CursorMismatch)
        );
    }
}

// R19, R29–R30
#[test]
fn h_cursor_weak_resume() {
    let mut s = Store::new();
    for k in ["b", "d", "f"] {
        s.set(0, k, "v", Some(2)).unwrap();
    }
    let p = s.scan(0, "", None, None, 0, 1, None).unwrap();
    s.delete(1, "b").unwrap();
    s.set(1, "a", "v", None).unwrap();
    s.set(1, "c", "v", None).unwrap();
    s.set(1, "e", "v", None).unwrap();
    let q = s.scan(2, "", None, None, 0, 9, p.next.as_ref()).unwrap();
    assert_eq!(
        q.items.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["c", "e"]
    );
}

// R3–R4, R8, R25
#[test]
fn h_scan_scopes() {
    let mut s = Store::new();
    s.set(0, "a", "1", None).unwrap();
    s.set(0, "b", "2", None).unwrap();
    s.begin(0).unwrap();
    s.delete(0, "a").unwrap();
    s.set(0, "b", "9", None).unwrap();
    s.set(0, "c", "3", None).unwrap();
    assert_eq!(
        s.scan(0, "", None, None, 0, 9, None).unwrap().items,
        vec![("b".into(), "9".into()), ("c".into(), "3".into())]
    );
    s.rollback(0).unwrap();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Ok(Aggregate { count: 2, sum: 3 })
    );
}

// R25–R26, R31–R32
#[test]
fn h_aggregate_empty_filtered() {
    let mut s = Store::new();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Ok(Aggregate { count: 0, sum: 0 })
    );
    s.set(0, "bad", "oops", None).unwrap();
    s.set(0, "num1", "0", None).unwrap();
    s.set(0, "num2", "-7", None).unwrap();
    assert_eq!(
        s.aggregate(0, "num", Some("num2"), Some("num")),
        Ok(Aggregate { count: 1, sum: 0 })
    );
    assert_eq!(
        s.aggregate(0, "num", None, Some("num")),
        Ok(Aggregate { count: 2, sum: -7 })
    );
}

// R32, R34
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
        s.set(0, "bad", v, None).unwrap();
        assert_eq!(
            s.aggregate(0, "", None, None),
            Err(Error::InvalidNumber {
                key: "bad".into(),
                value: v.into()
            })
        );
        assert_eq!(s.get(0, "bad"), Ok(Some(v.into())));
    }
}

// R31–R33
#[test]
fn h_numeric_limits() {
    let mut s = Store::new();
    s.set(0, "a", &i64::MIN.to_string(), None).unwrap();
    s.set(0, "b", &i64::MAX.to_string(), None).unwrap();
    s.set(0, "c", "1", None).unwrap();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Ok(Aggregate { count: 3, sum: 0 })
    );
}

// R33–R34
#[test]
fn h_sum_precedence() {
    let mut s = Store::new();
    s.set(0, "a", &i64::MAX.to_string(), None).unwrap();
    s.set(0, "b", "1", None).unwrap();
    s.set(0, "c", "nonsense", None).unwrap();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Err(Error::SumOverflow { key: "b".into() })
    );
    s.set(0, "a", "bad", None).unwrap();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Err(Error::InvalidNumber {
            key: "a".into(),
            value: "bad".into()
        })
    );
}

// R33
#[test]
fn h_negative_overflow() {
    let mut s = Store::new();
    s.set(0, "a", &i64::MIN.to_string(), None).unwrap();
    s.set(0, "b", "-1", None).unwrap();
    assert_eq!(
        s.aggregate(0, "", None, None),
        Err(Error::SumOverflow { key: "b".into() })
    );
}

// R19, R25, R31–R32
#[test]
fn h_expired_invalid_ignored() {
    let mut s = Store::new();
    s.set(0, "a", "bad", Some(2)).unwrap();
    s.set(0, "b", "6", None).unwrap();
    assert_eq!(
        s.aggregate(2, "", None, None),
        Ok(Aggregate { count: 1, sum: 6 })
    );
}

// R28–R30
#[test]
fn h_manual_cursor() {
    let mut s = Store::new();
    for k in ["aa", "cc", "ee"] {
        s.set(0, k, "v", None).unwrap();
    }
    let c = Cursor {
        start: "".into(),
        end: None,
        prefix: None,
        offset: usize::MAX,
        last: "bb".into(),
    };
    let p = s.scan(0, "", None, None, usize::MAX, 1, Some(&c)).unwrap();
    assert_eq!(p.items[0].0, "cc");
    assert_eq!(p.next.unwrap().last, "cc");
}
