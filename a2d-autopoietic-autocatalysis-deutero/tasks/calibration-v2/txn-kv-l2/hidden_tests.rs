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

// R19–R20
#[test]
fn h_deadline_boundary() {
    let mut s = Store::new();
    s.set(6, "edge", "live", Some(3)).unwrap();
    assert_eq!(s.get(8, "edge"), Ok(Some("live".into())));
    assert_eq!(s.get(9, "edge"), Ok(None));
    assert_eq!(s.delete(9, "edge"), Ok(false));
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

// R5, R19
#[test]
fn h_count_expiry() {
    let mut s = Store::new();
    s.set(0, "a", "", Some(4)).unwrap();
    s.set(0, "b", "", Some(5)).unwrap();
    s.set(0, "c", "", None).unwrap();
    assert_eq!(s.count(4, ""), Ok(2));
    assert_eq!(s.count(5, ""), Ok(1));
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

// R19–R20
#[test]
fn h_last_representable() {
    let mut s = Store::new();
    s.set(u64::MAX - 2, "edge", "v", Some(2)).unwrap();
    assert_eq!(s.get(u64::MAX - 1, "edge"), Ok(Some("v".into())));
    assert_eq!(s.get(u64::MAX, "edge"), Ok(None));
    s.set(u64::MAX, "zero", "v", Some(0)).unwrap();
    assert_eq!(s.get(u64::MAX, "zero"), Ok(None));
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

// R18, R24
#[test]
fn h_all_time_checks() {
    let mut s = Store::new();
    s.depth(8).unwrap();
    let e = Err(Error::TimeBackwards {
        previous: 8,
        now: 7,
    });
    assert_eq!(s.begin(7), e);
    assert_eq!(s.commit(7), e);
    assert_eq!(s.rollback(7), e);
    assert_eq!(s.set(7, "", "", None), e);
    assert_eq!(s.snapshot(7, ""), e);
    assert_eq!(s.restore(7, ""), e);
    assert_eq!(s.forget(7, ""), e);
    assert_eq!(
        s.delete(7, ""),
        Err(Error::TimeBackwards {
            previous: 8,
            now: 7
        })
    );
    assert_eq!(
        s.count(7, ""),
        Err(Error::TimeBackwards {
            previous: 8,
            now: 7
        })
    );
    assert_eq!(
        s.depth(7),
        Err(Error::TimeBackwards {
            previous: 8,
            now: 7
        })
    );
}

// R8, R19, R22
#[test]
fn h_rollback_deadline() {
    let mut s = Store::new();
    s.set(1, "k", "old", Some(5)).unwrap();
    s.begin(2).unwrap();
    s.set(2, "k", "new", None).unwrap();
    s.rollback(6).unwrap();
    assert_eq!(s.get(6, "k"), Ok(None));
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

// R6–R8, R19, R21–R22
#[test]
fn h_nested_expiry() {
    let mut s = Store::new();
    s.set(0, "k", "base", Some(7)).unwrap();
    s.begin(1).unwrap();
    s.begin(2).unwrap();
    assert_eq!(s.get(7, "k"), Ok(None));
    s.rollback(7).unwrap();
    assert_eq!(s.get(7, "k"), Ok(None));
    s.rollback(7).unwrap();
    assert_eq!(s.get(7, "k"), Ok(None));
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

// R14, R18
#[test]
fn h_clock_independent() {
    let mut a = Store::new();
    let mut b = Store::new();
    a.depth(100).unwrap();
    b.set(1, "x", "v", Some(1)).unwrap();
    assert_eq!(b.get(1, "x"), Ok(Some("v".into())));
}

// R6, R18–R19
#[test]
fn h_depth_purges() {
    let mut s = Store::new();
    s.set(0, "x", "v", Some(1)).unwrap();
    for _ in 0..4 {
        s.begin(0).unwrap();
    }
    assert_eq!(s.begin(1), Err(Error::DepthLimit));
    assert_eq!(s.count(1, "v"), Ok(0));
    assert_eq!(
        s.depth(0),
        Err(Error::TimeBackwards {
            previous: 1,
            now: 0
        })
    );
}
